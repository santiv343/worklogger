//! Routes Jira issue reads to the connection a request names.
//!
//! Only reads are routable. Every mutation reaches the primary connection, so
//! a write can never land on a site the caller did not select as its own, and
//! a caller that knows nothing about named connections keeps the exact
//! behaviour it had before they existed.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::jira_issues::{
    JiraAddCommentRequest, JiraEditMetadataData, JiraGetIssueRequest, JiraIssueBackend,
    JiraIssueBackendError, JiraIssueData, JiraIssueFuture, JiraIssueKeyRequest,
    JiraIssueSearchData, JiraMutationData, JiraMutationPlan, JiraMutationRequest,
    JiraSearchIssuesRequest, JiraTransitionIssueRequest, JiraTransitionsData,
    JiraUpdateIssueRequest,
};

/// Dispatches Jira issue reads across the configured connections.
pub struct RoutedJiraIssueBackend {
    primary: Arc<dyn JiraIssueBackend>,
    named: BTreeMap<String, Arc<dyn JiraIssueBackend>>,
}

impl RoutedJiraIssueBackend {
    /// Builds a backend that serves `named` connections alongside the primary.
    #[must_use]
    pub fn new(
        primary: Arc<dyn JiraIssueBackend>,
        named: BTreeMap<String, Arc<dyn JiraIssueBackend>>,
    ) -> Self {
        Self { primary, named }
    }

    fn route(
        &self,
        connection: Option<&str>,
    ) -> Result<&dyn JiraIssueBackend, JiraIssueBackendError> {
        match connection {
            None => Ok(self.primary.as_ref()),
            Some(name) => self
                .named
                .get(name)
                .map(AsRef::as_ref)
                .ok_or(JiraIssueBackendError::UnknownConnection),
        }
    }
}

fn rejected<'backend, Output: Send + 'backend>(
    error: JiraIssueBackendError,
) -> JiraIssueFuture<'backend, Output> {
    Box::pin(std::future::ready(Err(error)))
}

impl JiraIssueBackend for RoutedJiraIssueBackend {
    fn get_issue(&self, request: JiraGetIssueRequest) -> JiraIssueFuture<'_, JiraIssueData> {
        match self.route(request.connection.as_deref()) {
            Ok(backend) => backend.get_issue(request),
            Err(error) => rejected(error),
        }
    }

    fn search_issues(
        &self,
        request: JiraSearchIssuesRequest,
    ) -> JiraIssueFuture<'_, JiraIssueSearchData> {
        match self.route(request.connection.as_deref()) {
            Ok(backend) => backend.search_issues(request),
            Err(error) => rejected(error),
        }
    }

    fn get_edit_metadata(
        &self,
        request: JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, JiraEditMetadataData> {
        self.primary.get_edit_metadata(request)
    }

    fn get_transitions(
        &self,
        request: JiraIssueKeyRequest,
    ) -> JiraIssueFuture<'_, JiraTransitionsData> {
        self.primary.get_transitions(request)
    }

    fn update_issue(
        &self,
        request: JiraUpdateIssueRequest,
    ) -> JiraIssueFuture<'_, JiraMutationData> {
        self.primary.update_issue(request)
    }

    fn add_comment(&self, request: JiraAddCommentRequest) -> JiraIssueFuture<'_, JiraMutationData> {
        self.primary.add_comment(request)
    }

    fn transition_issue(
        &self,
        request: JiraTransitionIssueRequest,
    ) -> JiraIssueFuture<'_, JiraMutationData> {
        self.primary.transition_issue(request)
    }

    fn preview_mutation(
        &self,
        request: JiraMutationRequest,
    ) -> JiraIssueFuture<'_, JiraMutationPlan> {
        self.primary.preview_mutation(request)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Arc, BTreeMap, JiraAddCommentRequest, JiraEditMetadataData, JiraGetIssueRequest,
        JiraIssueBackend, JiraIssueBackendError, JiraIssueData, JiraIssueFuture,
        JiraIssueKeyRequest, JiraIssueSearchData, JiraMutationData, JiraMutationPlan,
        JiraMutationRequest, JiraSearchIssuesRequest, JiraTransitionIssueRequest,
        JiraTransitionsData, JiraUpdateIssueRequest, RoutedJiraIssueBackend, rejected,
    };

    /// Answers every call by failing with its own name, so a test can tell
    /// which backend a request actually reached.
    struct NamedBackend {
        label: &'static str,
    }

    impl NamedBackend {
        fn arc(label: &'static str) -> Arc<dyn JiraIssueBackend> {
            Arc::new(Self { label })
        }

        fn answer<Output: Send + 'static>(&self) -> JiraIssueFuture<'_, Output> {
            rejected(JiraIssueBackendError::ProviderRejected {
                detail: self.label.to_owned(),
            })
        }
    }

    impl JiraIssueBackend for NamedBackend {
        fn get_issue(&self, _request: JiraGetIssueRequest) -> JiraIssueFuture<'_, JiraIssueData> {
            self.answer()
        }

        fn search_issues(
            &self,
            _request: JiraSearchIssuesRequest,
        ) -> JiraIssueFuture<'_, JiraIssueSearchData> {
            self.answer()
        }

        fn get_edit_metadata(
            &self,
            _request: JiraIssueKeyRequest,
        ) -> JiraIssueFuture<'_, JiraEditMetadataData> {
            self.answer()
        }

        fn get_transitions(
            &self,
            _request: JiraIssueKeyRequest,
        ) -> JiraIssueFuture<'_, JiraTransitionsData> {
            self.answer()
        }

        fn update_issue(
            &self,
            _request: JiraUpdateIssueRequest,
        ) -> JiraIssueFuture<'_, JiraMutationData> {
            self.answer()
        }

        fn add_comment(
            &self,
            _request: JiraAddCommentRequest,
        ) -> JiraIssueFuture<'_, JiraMutationData> {
            self.answer()
        }

        fn transition_issue(
            &self,
            _request: JiraTransitionIssueRequest,
        ) -> JiraIssueFuture<'_, JiraMutationData> {
            self.answer()
        }

        fn preview_mutation(
            &self,
            _request: JiraMutationRequest,
        ) -> JiraIssueFuture<'_, JiraMutationPlan> {
            self.answer()
        }
    }

    fn routed() -> RoutedJiraIssueBackend {
        RoutedJiraIssueBackend::new(
            NamedBackend::arc("primary"),
            BTreeMap::from([("legacy".to_owned(), NamedBackend::arc("legacy"))]),
        )
    }

    fn reached(error: JiraIssueBackendError) -> String {
        match error {
            JiraIssueBackendError::ProviderRejected { detail } => detail,
            other => panic!("the call did not reach a backend: {other}"),
        }
    }

    fn get_issue(connection: Option<&str>) -> JiraGetIssueRequest {
        JiraGetIssueRequest {
            key: "PROJECT-1".to_owned(),
            fields: None,
            connection: connection.map(str::to_owned),
        }
    }

    fn search(connection: Option<&str>) -> JiraSearchIssuesRequest {
        JiraSearchIssuesRequest {
            jql: "ORDER BY updated DESC".to_owned(),
            fields: None,
            max_results: None,
            connection: connection.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn an_omitted_connection_reads_from_the_primary() {
        let error = routed()
            .get_issue(get_issue(None))
            .await
            .expect_err("the fake backend always fails");

        assert_eq!(reached(error), "primary");
    }

    #[tokio::test]
    async fn a_named_connection_reads_from_that_connection() {
        let error = routed()
            .get_issue(get_issue(Some("legacy")))
            .await
            .expect_err("the fake backend always fails");

        assert_eq!(reached(error), "legacy");
    }

    #[tokio::test]
    async fn a_search_follows_the_same_routing() {
        let backend = routed();

        let primary = backend
            .search_issues(search(None))
            .await
            .expect_err("the fake backend always fails");
        let legacy = backend
            .search_issues(search(Some("legacy")))
            .await
            .expect_err("the fake backend always fails");

        assert_eq!(reached(primary), "primary");
        assert_eq!(reached(legacy), "legacy");
    }

    #[tokio::test]
    async fn an_unknown_connection_fails_without_reaching_any_backend() {
        let error = routed()
            .get_issue(get_issue(Some("missing")))
            .await
            .expect_err("an unknown connection cannot be served");

        assert!(matches!(error, JiraIssueBackendError::UnknownConnection));
    }

    #[tokio::test]
    async fn writes_and_metadata_always_reach_the_primary() {
        let backend = routed();

        let metadata = backend
            .get_edit_metadata(JiraIssueKeyRequest {
                key: "PROJECT-1".to_owned(),
            })
            .await
            .expect_err("the fake backend always fails");
        let transitions = backend
            .get_transitions(JiraIssueKeyRequest {
                key: "PROJECT-1".to_owned(),
            })
            .await
            .expect_err("the fake backend always fails");

        assert_eq!(reached(metadata), "primary");
        assert_eq!(reached(transitions), "primary");
    }
}
