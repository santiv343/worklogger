//! Runtime projection of shared preferences; only explicit MCP grants enable tools.

use std::collections::BTreeMap;

use worklogger_settings::{JiraSettings, McpSettings, ModuleSettings, SettingsDocument};

use crate::{
    BitbucketConfiguration, BitbucketPullRequestDefaults, Capability, ConfigurationError,
    DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS, JiraConfiguration, JiraHoursConfiguration,
    McpConfiguration, ModuleConfiguration, ModuleId,
};

impl McpConfiguration {
    /// Projects complete settings into the MCP runtime without granting permissions.
    /// Incomplete enabled providers return `None` so settings remain editable.
    ///
    /// # Errors
    /// Rejects invalid shared or runtime configuration values.
    pub fn from_shared(document: &SettingsDocument) -> Result<Option<Self>, ConfigurationError> {
        document.validate()?;
        let modules = shared_modules(document);
        if !modules.values().any(|module| module.enabled) {
            return Ok(None);
        }
        let jira = project_jira(document);
        let bitbucket = project_bitbucket(document);
        if !runtime_complete(&modules, jira.as_ref(), bitbucket.as_ref()) {
            return Ok(None);
        }
        let mut configuration = Self::build(jira, bitbucket, modules)?;
        configuration.jira_connections = project_connections(document);
        Ok(Some(configuration))
    }

    /// Imports explicit MCP settings while preserving preferences owned by Desktop.
    ///
    /// # Errors
    /// Rejects invalid configuration or serialization failures.
    pub fn update_shared(&self, document: &mut SettingsDocument) -> Result<(), ConfigurationError> {
        if let Some(jira) = &self.jira {
            update_jira(document, jira);
            update_hours(document, jira.hours.as_ref());
        }
        update_connections(document, &self.jira_connections);
        if let Some(bitbucket) = &self.bitbucket {
            document.bitbucket = Some(worklogger_settings::BitbucketSettings {
                email: Some(bitbucket.email.clone()),
                workspaces: Some(bitbucket.workspaces.clone()),
                request_timeout_seconds: Some(bitbucket.request_timeout_seconds),
                page_size: Some(bitbucket.page_size),
                maximum_collection_items: Some(bitbucket.maximum_collection_items),
                pull_request_defaults: Some(worklogger_settings::PullRequestDefaults {
                    reviewer_account_ids: bitbucket
                        .pull_request_defaults
                        .reviewer_account_ids
                        .clone(),
                    close_source_branch: bitbucket.pull_request_defaults.close_source_branch,
                }),
            });
        }
        document.mcp = Some(McpSettings {
            modules: self
                .modules
                .iter()
                .map(|(module_id, module)| {
                    (
                        *module_id,
                        ModuleSettings {
                            enabled: module.enabled,
                            capabilities: module.capabilities.clone(),
                        },
                    )
                })
                .collect(),
        });
        document.validate()?;
        Ok(())
    }
}

fn shared_modules(document: &SettingsDocument) -> BTreeMap<ModuleId, ModuleConfiguration> {
    document.mcp.as_ref().map_or_else(BTreeMap::new, |mcp| {
        mcp.modules
            .iter()
            .map(|(module_id, module)| {
                (
                    *module_id,
                    ModuleConfiguration {
                        enabled: module.enabled,
                        capabilities: module.capabilities.clone(),
                    },
                )
            })
            .collect()
    })
}

fn project_jira(document: &SettingsDocument) -> Option<JiraConfiguration> {
    let jira = document.jira.as_ref()?;
    Some(JiraConfiguration {
        base_url: jira.base_url.clone()?,
        email: jira.email.clone()?,
        board_id: jira.board_id?,
        request_timeout_seconds: jira.request_timeout_seconds?,
        page_size: jira.page_size?,
        maximum_collection_items: jira.maximum_collection_items?,
        maximum_issue_search_results: jira.maximum_issue_search_results?,
        hours: project_hours(document),
    })
}

/// Projects the additional connections the runtime can actually serve.
///
/// A connection the shared document accepts as a draft but the runtime cannot
/// use is skipped rather than failing the whole projection: a half-configured
/// secondary connection must not stop the primary one from working.
fn project_connections(document: &SettingsDocument) -> BTreeMap<String, JiraConfiguration> {
    document
        .jira_connections
        .iter()
        .filter_map(|(name, settings)| {
            let connection = project_connection(settings)?;
            if connection.is_usable() {
                Some((name.clone(), connection))
            } else {
                None
            }
        })
        .collect()
}

/// Projects one additional connection.
///
/// Hours are deliberately left out: only issue reads are routed to a named
/// connection, so a secondary site never needs the time-entry limits, and the
/// shared `hours` section describes the person rather than any one site.
fn project_connection(settings: &JiraSettings) -> Option<JiraConfiguration> {
    Some(JiraConfiguration {
        base_url: settings.base_url.clone()?,
        email: settings.email.clone()?,
        board_id: settings.board_id?,
        request_timeout_seconds: settings.request_timeout_seconds?,
        page_size: settings.page_size?,
        maximum_collection_items: settings.maximum_collection_items?,
        maximum_issue_search_results: settings.maximum_issue_search_results?,
        hours: None,
    })
}

/// Writes the known connections back without disturbing the rest.
///
/// This merges instead of replacing on purpose. A connection that
/// `project_connections` skipped is absent from the runtime, and replacing the
/// map wholesale would delete it from the user's settings file. The Desktop's
/// `activeConnection` is left untouched for the same reason.
fn update_connections(
    document: &mut SettingsDocument,
    connections: &BTreeMap<String, JiraConfiguration>,
) {
    for (name, connection) in connections {
        let maximum_concurrent_worklog_requests = document
            .jira_connections
            .get(name)
            .and_then(|current| current.maximum_concurrent_worklog_requests);
        document.jira_connections.insert(
            name.clone(),
            JiraSettings {
                base_url: Some(connection.base_url.clone()),
                email: Some(connection.email.clone()),
                board_id: Some(connection.board_id),
                request_timeout_seconds: Some(connection.request_timeout_seconds),
                page_size: Some(connection.page_size),
                maximum_collection_items: Some(connection.maximum_collection_items),
                maximum_issue_search_results: Some(connection.maximum_issue_search_results),
                maximum_concurrent_worklog_requests,
            },
        );
    }
}

fn project_hours(document: &SettingsDocument) -> Option<JiraHoursConfiguration> {
    let hours = document.hours.as_ref()?;
    Some(JiraHoursConfiguration {
        weekly_target_hours: hours.weekly_target_hours?,
        utc_offset_minutes: hours.utc_offset_minutes?,
        maximum_daily_hours: hours.maximum_daily_hours.unwrap_or(24),
        maximum_report_period_days: hours
            .maximum_report_period_days
            .unwrap_or(DEFAULT_MAXIMUM_REPORT_PERIOD_DAYS),
        maximum_concurrent_worklog_requests: document
            .jira
            .as_ref()?
            .maximum_concurrent_worklog_requests?,
    })
}

fn project_bitbucket(document: &SettingsDocument) -> Option<BitbucketConfiguration> {
    let bitbucket = document.bitbucket.as_ref()?;
    Some(BitbucketConfiguration {
        email: bitbucket.email.clone()?,
        workspaces: bitbucket.workspaces.clone()?,
        request_timeout_seconds: bitbucket.request_timeout_seconds?,
        page_size: bitbucket.page_size?,
        maximum_collection_items: bitbucket.maximum_collection_items?,
        pull_request_defaults: bitbucket.pull_request_defaults.as_ref().map_or_else(
            BitbucketPullRequestDefaults::default,
            |defaults| BitbucketPullRequestDefaults {
                reviewer_account_ids: defaults.reviewer_account_ids.clone(),
                close_source_branch: defaults.close_source_branch,
            },
        ),
    })
}

fn runtime_complete(
    modules: &BTreeMap<ModuleId, ModuleConfiguration>,
    jira: Option<&JiraConfiguration>,
    bitbucket: Option<&BitbucketConfiguration>,
) -> bool {
    modules.iter().all(|(module_id, module)| {
        if !module.enabled {
            return true;
        }
        match module_id {
            ModuleId::Jira => jira.is_some_and(|jira| {
                !module
                    .capabilities
                    .contains(&Capability::ReadOwnTimeEntries)
                    || jira.hours.is_some()
            }),
            ModuleId::Bitbucket => bitbucket.is_some(),
        }
    })
}

fn update_jira(document: &mut SettingsDocument, jira: &JiraConfiguration) {
    let maximum_concurrent_worklog_requests = jira.hours.as_ref().map_or_else(
        || {
            document
                .jira
                .as_ref()
                .and_then(|current| current.maximum_concurrent_worklog_requests)
        },
        |hours| Some(hours.maximum_concurrent_worklog_requests),
    );
    document.jira = Some(JiraSettings {
        base_url: Some(jira.base_url.clone()),
        email: Some(jira.email.clone()),
        board_id: Some(jira.board_id),
        request_timeout_seconds: Some(jira.request_timeout_seconds),
        page_size: Some(jira.page_size),
        maximum_collection_items: Some(jira.maximum_collection_items),
        maximum_issue_search_results: Some(jira.maximum_issue_search_results),
        maximum_concurrent_worklog_requests,
    });
}

fn update_hours(document: &mut SettingsDocument, hours: Option<&JiraHoursConfiguration>) {
    let Some(hours) = hours else {
        return;
    };
    let current = document.hours.get_or_insert_with(Default::default);
    current.weekly_target_hours = Some(hours.weekly_target_hours);
    current.utc_offset_minutes = Some(hours.utc_offset_minutes);
    current.maximum_daily_hours = Some(hours.maximum_daily_hours);
    current.maximum_report_period_days = Some(hours.maximum_report_period_days);
}
