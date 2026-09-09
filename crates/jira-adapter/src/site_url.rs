use hours_core::IssueKey;
use reqwest::Url;

use crate::JiraError;

const ATLASSIAN_SUFFIX: &str = ".atlassian.net";
const BROWSE_SEGMENT: &str = "browse";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JiraSiteUrl(Url);

impl JiraSiteUrl {
    /// Parses a Jira Cloud tenant origin allowed to receive credentials.
    ///
    /// # Errors
    ///
    /// Returns [`JiraError::InvalidSiteUrl`] for any non-tenant origin.
    pub fn parse(value: &str) -> Result<Self, JiraError> {
        let normalized = value.trim();
        if has_explicit_port(normalized) {
            return Err(JiraError::InvalidSiteUrl);
        }
        let url = Url::parse(normalized).map_err(|_| JiraError::InvalidSiteUrl)?;
        validate_url_shape(&url)?;
        validate_tenant_host(&url)?;
        Ok(Self(url))
    }

    #[must_use]
    pub fn as_url(&self) -> &Url {
        &self.0
    }

    #[must_use]
    pub fn issue_browser_url(&self, issue_key: &IssueKey) -> String {
        format!("{}{BROWSE_SEGMENT}/{}", self.0, issue_key.as_str())
    }

    #[cfg(test)]
    pub(crate) fn loopback_for_test(value: &str) -> Result<Self, JiraError> {
        let url = Url::parse(value).map_err(|_| JiraError::InvalidSiteUrl)?;
        let host = url.host_str().ok_or(JiraError::InvalidSiteUrl)?;
        let is_loopback = host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
        let is_origin = url.path() == "/" && url.query().is_none() && url.fragment().is_none();
        let has_no_credentials = url.username().is_empty() && url.password().is_none();
        if url.scheme() != "http" || !is_loopback || !is_origin || !has_no_credentials {
            return Err(JiraError::InvalidSiteUrl);
        }
        Ok(Self(url))
    }
}

fn has_explicit_port(value: &str) -> bool {
    let authority = value
        .strip_prefix("https://")
        .unwrap_or(value)
        .split('/')
        .next()
        .unwrap_or_default();
    authority.rsplit_once(':').is_some()
}

fn validate_url_shape(url: &Url) -> Result<(), JiraError> {
    let is_origin = url.path() == "/" && url.query().is_none() && url.fragment().is_none();
    let has_no_credentials = url.username().is_empty() && url.password().is_none();
    if url.scheme() != "https" || url.port().is_some() || !is_origin || !has_no_credentials {
        return Err(JiraError::InvalidSiteUrl);
    }
    Ok(())
}

fn validate_tenant_host(url: &Url) -> Result<(), JiraError> {
    let host = url.host_str().ok_or(JiraError::InvalidSiteUrl)?;
    let tenant = host
        .strip_suffix(ATLASSIAN_SUFFIX)
        .ok_or(JiraError::InvalidSiteUrl)?;
    if !is_valid_tenant(tenant) {
        return Err(JiraError::InvalidSiteUrl);
    }
    Ok(())
}

fn is_valid_tenant(tenant: &str) -> bool {
    let boundary_is_alphanumeric =
        tenant.starts_with(char::is_alphanumeric) && tenant.ends_with(char::is_alphanumeric);
    boundary_is_alphanumeric
        && tenant
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

#[cfg(test)]
mod tests {
    use super::JiraSiteUrl;

    #[test]
    fn accepts_a_plain_atlassian_tenant_origin() {
        let site = JiraSiteUrl::parse(" https://example.atlassian.net ").expect("valid site");
        assert_eq!(site.as_url().as_str(), "https://example.atlassian.net/");
    }

    #[test]
    fn rejects_hosts_that_could_receive_the_token() {
        let invalid = [
            "http://example.atlassian.net",
            "https://example.atlassian.net.evil.test",
            "https://nested.example.atlassian.net",
            "https://user@example.atlassian.net",
            "https://example.atlassian.net:443",
            "https://example.atlassian.net/rest/api/3/myself",
        ];
        for value in invalid {
            assert!(JiraSiteUrl::parse(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn test_origin_accepts_only_http_loopback() {
        assert!(JiraSiteUrl::loopback_for_test("http://127.0.0.1:9000").is_ok());
        assert!(JiraSiteUrl::loopback_for_test("http://example.test:9000").is_err());
    }
}
