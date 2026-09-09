use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::Serialize;
use thiserror::Error;

const CONFIRMATION_TTL_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_MCP_CONFIRMATION_TTL_SECONDS";
const MAXIMUM_PENDING_ENVIRONMENT_VARIABLE: &str = "WORKLOGGER_MCP_MAX_PENDING_CONFIRMATIONS";
const DEFAULT_CONFIRMATION_TTL_SECONDS: u64 = 300;
const DEFAULT_MAXIMUM_PENDING_CONFIRMATIONS: usize = 128;
const CONFIRMED_PROPERTY: &str = "confirmed";
const CONFIRMATION_TOKEN_PROPERTY: &str = "confirmationToken";

#[derive(Debug, Error)]
pub enum ConfirmationError {
    #[error("the confirmation does not exist, has expired, or was already used")]
    MissingOrExpired,
    #[error("the operation changed since the preview")]
    PayloadChanged,
    #[error("could not prepare the confirmation")]
    Unavailable,
}

#[derive(Clone, Debug)]
struct PendingConfirmation {
    payload: Vec<u8>,
    expires_at: Instant,
}

#[derive(Debug)]
pub struct ConfirmationGate {
    pending: Mutex<BTreeMap<String, PendingConfirmation>>,
    sequence: AtomicU64,
    ttl: Duration,
    maximum_pending: usize,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MutationConfirmation<Preview> {
    pub token: String,
    pub preview: Preview,
    /// Exact preview formatted for direct display in the conversation.
    pub visible_preview: String,
}

impl Default for ConfirmationGate {
    fn default() -> Self {
        Self {
            pending: Mutex::new(BTreeMap::new()),
            sequence: AtomicU64::new(0),
            ttl: Duration::from_secs(environment_u64(
                CONFIRMATION_TTL_ENVIRONMENT_VARIABLE,
                DEFAULT_CONFIRMATION_TTL_SECONDS,
            )),
            maximum_pending: environment_usize(
                MAXIMUM_PENDING_ENVIRONMENT_VARIABLE,
                DEFAULT_MAXIMUM_PENDING_CONFIRMATIONS,
            ),
        }
    }
}

impl ConfirmationGate {
    /// Creates a single-use token for one exact serialized mutation payload.
    ///
    /// # Errors
    ///
    /// Returns an error when the confirmation store or token source is unavailable.
    pub fn prepare(&self, payload: Vec<u8>) -> Result<String, ConfirmationError> {
        let mut pending = self.pending()?;
        remove_expired(&mut pending);
        make_room(&mut pending, self.maximum_pending);
        let token = self.next_token()?;
        let confirmation = PendingConfirmation {
            payload,
            expires_at: Instant::now() + self.ttl,
        };
        pending.insert(token.clone(), confirmation);
        Ok(token)
    }

    /// Consumes a token only when its payload is unchanged and has not expired.
    ///
    /// # Errors
    ///
    /// Returns an error for expired, reused, changed or unavailable confirmations.
    pub fn consume(&self, token: &str, payload: &[u8]) -> Result<(), ConfirmationError> {
        let mut pending = self.pending()?;
        remove_expired(&mut pending);
        let confirmation = pending
            .remove(token)
            .ok_or(ConfirmationError::MissingOrExpired)?;
        if confirmation.payload == payload {
            return Ok(());
        }
        Err(ConfirmationError::PayloadChanged)
    }

    fn pending(
        &self,
    ) -> Result<MutexGuard<'_, BTreeMap<String, PendingConfirmation>>, ConfirmationError> {
        self.pending
            .lock()
            .map_err(|_| ConfirmationError::Unavailable)
    }

    fn next_token(&self) -> Result<String, ConfirmationError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ConfirmationError::Unavailable)?
            .as_nanos();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        Ok(format!(
            "{:x}-{timestamp:x}-{sequence:x}",
            std::process::id()
        ))
    }
}

/// Serializes a mutation request without its confirmation control fields.
///
/// # Errors
///
/// Returns an error when the request cannot be represented as a JSON object.
pub fn confirmation_payload<Request: Serialize>(
    request: &Request,
) -> Result<Vec<u8>, ConfirmationError> {
    confirmation_payload_with_context(request, &())
}

/// Serializes a mutation request together with the provider state shown in its preview.
///
/// # Errors
///
/// Returns an error when the request or preview cannot be serialized safely.
pub fn confirmation_payload_with_context<Request: Serialize, Context: Serialize>(
    request: &Request,
    context: &Context,
) -> Result<Vec<u8>, ConfirmationError> {
    let mut value = serde_json::to_value(request).map_err(|_| ConfirmationError::Unavailable)?;
    let object = value
        .as_object_mut()
        .ok_or(ConfirmationError::Unavailable)?;
    object.remove(CONFIRMED_PROPERTY);
    object.remove(CONFIRMATION_TOKEN_PROPERTY);
    serde_json::to_vec(&(value, context)).map_err(|_| ConfirmationError::Unavailable)
}

fn remove_expired(pending: &mut BTreeMap<String, PendingConfirmation>) {
    let now = Instant::now();
    pending.retain(|_, confirmation| confirmation.expires_at > now);
}

fn make_room(pending: &mut BTreeMap<String, PendingConfirmation>, maximum: usize) {
    while pending.len() >= maximum {
        let Some(oldest_key) = pending.keys().next().cloned() else {
            return;
        };
        pending.remove(&oldest_key);
    }
}

fn environment_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn environment_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::{ConfirmationError, ConfirmationGate, confirmation_payload};

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Request<'value> {
        target: &'value str,
        confirmed: bool,
        confirmation_token: Option<&'value str>,
    }

    #[test]
    fn confirmation_is_single_use_and_bound_to_the_payload() {
        let gate = ConfirmationGate::default();
        let first = request("DEMO-1");
        let payload = confirmation_payload(&first).expect("payload");
        let token = gate.prepare(payload.clone()).expect("token");

        let changed = confirmation_payload(&request("DEMO-2")).expect("changed payload");
        assert!(matches!(
            gate.consume(&token, &changed),
            Err(ConfirmationError::PayloadChanged)
        ));
        assert!(matches!(
            gate.consume(&token, &payload),
            Err(ConfirmationError::MissingOrExpired)
        ));
    }

    fn request(target: &str) -> Request<'_> {
        Request {
            target,
            confirmed: false,
            confirmation_token: None,
        }
    }
}
