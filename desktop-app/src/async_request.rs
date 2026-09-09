use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AsyncRequestId(u64);

impl AsyncRequestId {
    pub(crate) fn next() -> Self {
        Self(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::AsyncRequestId;

    #[test]
    fn request_ids_are_unique() {
        assert_ne!(AsyncRequestId::next(), AsyncRequestId::next());
    }
}
