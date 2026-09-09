use super::{
    AppSettings, HoursSettings, JiraSettings, ReportsSettings, SETTINGS_SCHEMA_VERSION,
    SettingsError,
};
use crate::defaults::product_defaults;
use worklogger_settings::SettingsDocument;

impl AppSettings {
    pub(super) fn from_shared(document: &SettingsDocument) -> Result<Option<Self>, SettingsError> {
        let Some(jira) = document.jira.as_ref().and_then(project_jira) else {
            return Ok(None);
        };
        let settings = Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            jira,
            hours: project_hours(document.hours.as_ref())?,
            reports: ReportsSettings {
                enable_team_reports: document
                    .reports
                    .as_ref()
                    .and_then(|reports| reports.enable_team_reports)
                    .unwrap_or(false),
            },
        };
        settings.validate()?;
        Ok(Some(settings))
    }

    pub(super) fn update_shared(
        &self,
        document: &mut SettingsDocument,
    ) -> Result<(), SettingsError> {
        document.jira = Some(worklogger_settings::JiraSettings {
            base_url: Some(self.jira.base_url.clone()),
            email: Some(self.jira.email.clone()),
            board_id: Some(self.jira.board_id),
            request_timeout_seconds: Some(self.jira.request_timeout_seconds),
            page_size: Some(self.jira.page_size),
            maximum_collection_items: Some(self.jira.maximum_collection_items),
            maximum_issue_search_results: Some(self.jira.maximum_issue_search_results),
            maximum_concurrent_worklog_requests: Some(
                self.jira.maximum_concurrent_worklog_requests,
            ),
        });
        document.hours = Some(worklogger_settings::HoursSettings {
            weekly_target_hours: Some(self.hours.weekly_target),
            utc_offset_minutes: Some(self.hours.utc_offset_minutes),
            maximum_daily_hours: Some(self.hours.maximum_daily_hours),
            default_worklog_start_hour: Some(self.hours.default_worklog_start_hour),
            default_worklog_start_minute: Some(self.hours.default_worklog_start_minute),
        });
        document.reports = Some(worklogger_settings::ReportsSettings {
            enable_team_reports: Some(self.reports.enable_team_reports),
        });
        document.validate()?;
        Ok(())
    }
}

fn project_jira(jira: &worklogger_settings::JiraSettings) -> Option<JiraSettings> {
    let defaults = product_defaults();
    let limits = defaults.jira();
    Some(JiraSettings {
        base_url: jira.base_url.clone()?,
        email: jira.email.clone()?,
        board_id: jira.board_id?,
        request_timeout_seconds: jira
            .request_timeout_seconds
            .unwrap_or(limits.request_timeout_seconds),
        page_size: jira.page_size.unwrap_or(limits.page_size),
        maximum_collection_items: jira
            .maximum_collection_items
            .unwrap_or(limits.maximum_collection_items),
        maximum_issue_search_results: jira
            .maximum_issue_search_results
            .unwrap_or(limits.maximum_issue_search_results),
        maximum_concurrent_worklog_requests: jira
            .maximum_concurrent_worklog_requests
            .unwrap_or(limits.maximum_concurrent_worklog_requests),
    })
}

fn project_hours(
    hours: Option<&worklogger_settings::HoursSettings>,
) -> Result<HoursSettings, SettingsError> {
    let defaults = product_defaults();
    let limits = defaults.hours();
    let empty = worklogger_settings::HoursSettings::default();
    let hours = hours.unwrap_or(&empty);
    let suggested = u16::try_from(limits.suggested_weekly_target_hours)
        .map_err(|_| super::invalid("default weekly target is too large"))?;
    Ok(HoursSettings {
        weekly_target: hours.weekly_target_hours.unwrap_or(suggested),
        utc_offset_minutes: hours
            .utc_offset_minutes
            .unwrap_or(limits.suggested_utc_offset_minutes),
        maximum_daily_hours: hours
            .maximum_daily_hours
            .unwrap_or(limits.maximum_daily_hours),
        default_worklog_start_hour: hours
            .default_worklog_start_hour
            .unwrap_or(limits.default_worklog_start_hour),
        default_worklog_start_minute: hours
            .default_worklog_start_minute
            .unwrap_or(limits.default_worklog_start_minute),
        legacy_enable_team_reports: None,
    })
}
