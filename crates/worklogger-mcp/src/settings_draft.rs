//! Editor values for incomplete settings held in the active TUI session.
//! Persistence belongs exclusively to `worklogger-settings`.

use std::collections::{BTreeMap, BTreeSet};

use worklogger_mcp::{BitbucketPullRequestDefaults, Capability, JiraHoursConfiguration};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct JiraSettingsDraft {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub board_id: Option<u64>,
    pub capabilities: BTreeSet<Capability>,
    pub hours: Option<JiraHoursConfiguration>,
    pub request_timeout_seconds: Option<u64>,
    pub page_size: Option<u16>,
    pub maximum_collection_items: Option<usize>,
    pub maximum_issue_search_results: Option<usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BitbucketSettingsDraft {
    pub email: Option<String>,
    pub workspaces: BTreeMap<String, BTreeSet<String>>,
    pub capabilities: BTreeSet<Capability>,
    pub pull_request_defaults: BitbucketPullRequestDefaults,
    pub request_timeout_seconds: Option<u64>,
    pub page_size: Option<u16>,
    pub maximum_collection_items: Option<usize>,
}
