//! Independent Jira settings editors. Incomplete connections never enter mcp.json.

use super::{
    BTreeSet, Capability, CliError, ConfigurationStore, CredentialPurpose, CredentialStore,
    DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS, Duration, JiraClient, JiraHoursConfiguration,
    JiraModuleProfile, JiraSetupValues, JiraSiteUrl, McpConfiguration, ModuleId,
    OrganizationProfile, ProviderRequestLimits, apply_setup_profile, capability_available, choose,
    choose_board, choose_jira_site, configured_jira, credential_transaction, default_jira_hours,
    default_provider_request_limits, disable_dependent_jira_capabilities, discover,
    environment_token, format_utc_offset, jira_profile_limits, jira_request_limits_allowed,
    message, profile_jira_hours, prompt, prompt_utc_offset, prompt_weekly_target, read_token,
    scoped_jira_boards, select_capabilities, settings_copy, settings_draft, show_message,
    show_progress, terminal_error, tui_copy, validate_prompted_hours,
};
use settings_copy::settings_copy;
use settings_draft::JiraSettingsDraft;
use worklogger_settings::{
    HoursSettings, JiraSettings, McpSettings, ModuleSettings, SettingsStore,
};

#[derive(Clone, Copy)]
enum Page {
    Jira,
    Connection,
    Hours,
    HoursSettings,
    Issues,
    Advanced,
}

struct Editor<'profile> {
    values: JiraSettingsDraft,
    profile: Option<&'profile OrganizationProfile>,
    verified: bool,
    board_verified: bool,
}

pub(super) async fn run(profile: Option<&OrganizationProfile>) -> Result<(), CliError> {
    let mut editor = Editor::load(profile)?;
    let mut pages = vec![Page::Jira];
    while let Some(page) = pages.last().copied() {
        match editor.step(page, &mut pages).await {
            Ok(()) => {}
            Err(CliError::Cancelled) => {
                pages.pop();
            }
            Err(error) => {
                show_message(&tui_copy().error_title, &[error.to_string()])
                    .map_err(|error| terminal_error(&error))?;
            }
        }
    }
    Ok(())
}

impl<'profile> Editor<'profile> {
    fn load(profile: Option<&'profile OrganizationProfile>) -> Result<Self, CliError> {
        let settings =
            SettingsStore::for_current_user().map_err(|error| message(error.to_string()))?;
        let document = settings
            .load()
            .map_err(|error| message(error.to_string()))?;
        let values = from_shared(document.as_ref(), profile);
        Ok(Self {
            values,
            profile,
            verified: false,
            board_verified: false,
        })
    }

    fn policy(&self) -> Option<&JiraModuleProfile> {
        self.profile
            .and_then(|profile| profile.modules.jira.as_ref())
    }

    async fn step(&mut self, page: Page, pages: &mut Vec<Page>) -> Result<(), CliError> {
        let options = self.options(page);
        let index = choose(&page_title(page), &options).map_err(|error| terminal_error(&error))?;
        if index == options.len() - 1 {
            pages.pop();
            return Ok(());
        }
        match page {
            Page::Jira => self.open_section(index, pages).await,
            Page::Connection => self.edit_connection(index).await,
            Page::Hours => self.open_hours(index, pages),
            Page::HoursSettings => self.edit_hours(index),
            Page::Issues => self.edit_issues(index),
            Page::Advanced => self.edit_advanced(index),
        }
    }

    fn options(&self, page: Page) -> Vec<String> {
        let mut options = match page {
            Page::Jira => self.jira_options(),
            Page::Connection => self.connection_options(),
            Page::Hours => self.hours_options(),
            Page::HoursSettings => self.hours_settings_options(),
            Page::Issues => self.issue_options(),
            Page::Advanced => self.advanced_options(),
        };
        options.push(settings_copy().back.clone());
        options
    }

    fn jira_options(&self) -> Vec<String> {
        let copy = settings_copy();
        vec![
            row(&copy.connection, self.connection_state()),
            row(&copy.board, &self.board_state()),
            row(
                &copy.hours,
                self.capability_state(Capability::ReadOwnTimeEntries),
            ),
            row(
                &copy.issues,
                self.capability_state(Capability::ReadJiraIssues),
            ),
            row(&copy.advanced, self.managed_state()),
        ]
    }

    fn connection_options(&self) -> Vec<String> {
        let copy = settings_copy();
        vec![
            row(
                &copy.site,
                self.values.base_url.as_deref().unwrap_or(&copy.needs_setup),
            ),
            row(
                &copy.email,
                self.values.email.as_deref().unwrap_or(&copy.needs_setup),
            ),
            row(
                &copy.token,
                if self.token().is_ok() {
                    &copy.saved
                } else {
                    &copy.needs_setup
                },
            ),
            row(&copy.verify, self.connection_state()),
        ]
    }

    fn hours_options(&self) -> Vec<String> {
        let copy = settings_copy();
        vec![
            row(
                &copy.settings,
                if self.values.hours.is_some() {
                    &copy.ready
                } else {
                    &copy.needs_setup
                },
            ),
            row(
                &copy.permissions,
                self.capability_state(Capability::ReadOwnTimeEntries),
            ),
        ]
    }

    fn hours_settings_options(&self) -> Vec<String> {
        let hours = self.hours_defaults();
        vec![
            row(
                &tui_copy().weekly_target_hours,
                &hours.weekly_target_hours.to_string(),
            ),
            row(
                &tui_copy().utc_offset,
                &format_utc_offset(hours.utc_offset_minutes).unwrap_or_default(),
            ),
            row(
                &settings_copy().concurrency,
                &hours.maximum_concurrent_worklog_requests.to_string(),
            ),
        ]
    }

    fn issue_options(&self) -> Vec<String> {
        vec![
            row(
                &settings_copy().permissions,
                self.capability_state(Capability::ReadJiraIssues),
            ),
            row(
                &settings_copy().search_limit,
                &self.search_limit().to_string(),
            ),
        ]
    }

    fn advanced_options(&self) -> Vec<String> {
        let limits = self.limits();
        vec![
            row(
                &settings_copy().timeout,
                &limits.timeout_seconds.to_string(),
            ),
            row(&settings_copy().page_size, &limits.page_size.to_string()),
            row(
                &settings_copy().collection_limit,
                &limits.maximum_collection_items.to_string(),
            ),
        ]
    }

    async fn open_section(&mut self, index: usize, pages: &mut Vec<Page>) -> Result<(), CliError> {
        match index {
            0 => pages.push(Page::Connection),
            1 => return self.edit_board().await,
            2 => pages.push(Page::Hours),
            3 => pages.push(Page::Issues),
            _ => pages.push(Page::Advanced),
        }
        Ok(())
    }

    fn open_hours(&mut self, index: usize, pages: &mut Vec<Page>) -> Result<(), CliError> {
        if index == 0 {
            pages.push(Page::HoursSettings);
            return Ok(());
        }
        self.edit_permissions(true)
    }

    async fn edit_connection(&mut self, index: usize) -> Result<(), CliError> {
        match index {
            0 => self.edit_site()?,
            1 => self.edit_email()?,
            2 => return self.replace_token().await,
            _ => return self.verify_connection().await,
        }
        self.save_draft()?;
        show_notice(&settings_copy().connection_changed)
    }

    fn edit_site(&mut self) -> Result<(), CliError> {
        let site = if self.policy().is_some_and(|policy| {
            policy.scope_mode == worklogger_profile::ProviderScopeMode::Restricted
        }) {
            choose_jira_site(self.policy())?
        } else {
            prompt(&settings_copy().site, self.values.base_url.as_deref())?
        };
        JiraSiteUrl::parse(&site).map_err(|error| message(error.to_string()))?;
        if self.values.base_url.as_ref() != Some(&site) {
            self.values.base_url = Some(site);
            self.invalidate_connection();
        }
        Ok(())
    }

    fn edit_email(&mut self) -> Result<(), CliError> {
        let email = prompt(&settings_copy().email, self.values.email.as_deref())?;
        if !email.contains('@') {
            return Err(message(&settings_copy().connection_required));
        }
        if self.values.email.as_ref() != Some(&email) {
            self.values.email = Some(email);
            self.invalidate_connection();
        }
        Ok(())
    }

    fn invalidate_connection(&mut self) {
        self.verified = false;
        self.board_verified = false;
        self.values.board_id = None;
    }

    async fn replace_token(&mut self) -> Result<(), CliError> {
        self.coordinates()?;
        let token = read_token()?;
        self.check_token(&token).await?;
        let (site, email) = self.coordinates()?;
        let _guard = credential_transaction()?;
        CredentialStore::for_purpose(CredentialPurpose::Mcp)
            .and_then(|store| store.save_api_token(site, email, &token))
            .map_err(|error| message(error.to_string()))?;
        self.verified = true;
        self.save_draft()?;
        show_notice(&settings_copy().verified)
    }

    async fn verify_connection(&mut self) -> Result<(), CliError> {
        self.verified = false;
        let token = self.token()?;
        self.check_token(&token).await?;
        self.verified = true;
        show_notice(&settings_copy().verified)
    }

    async fn check_token(&self, token: &str) -> Result<(), CliError> {
        let (site, email) = self.coordinates()?;
        let site = JiraSiteUrl::parse(site).map_err(|error| message(error.to_string()))?;
        let client = JiraClient::new(
            site,
            email,
            token,
            Duration::from_secs(self.limits().timeout_seconds),
        )
        .map_err(|error| message(error.to_string()))?;
        show_progress(&tui_copy().working_title, &tui_copy().validating_account)
            .map_err(|error| terminal_error(&error))?;
        client
            .current_user()
            .await
            .map_err(|error| message(error.to_string()))?;
        Ok(())
    }

    fn coordinates(&self) -> Result<(&str, &str), CliError> {
        self.values
            .base_url
            .as_deref()
            .zip(self.values.email.as_deref())
            .ok_or_else(|| message(&settings_copy().connection_required))
    }

    fn token(&self) -> Result<String, CliError> {
        let (site, email) = self.coordinates()?;
        if let Some(token) = environment_token() {
            return Ok(token);
        }
        CredentialStore::for_purpose(CredentialPurpose::Mcp)
            .map_err(|error| message(error.to_string()))?
            .load_api_token(site, email)
            .map_err(|error| message(error.to_string()))?
            .ok_or(CliError::MissingJiraToken)
    }

    async fn edit_board(&mut self) -> Result<(), CliError> {
        let (site, email) = self.coordinates()?;
        let token = self.token()?;
        let (_, boards) = discover(site, email, &token, self.limits()).await?;
        let boards = scoped_jira_boards(self.policy(), site, boards);
        let board = choose_board(&boards)?;
        self.values.board_id = Some(board);
        self.verified = true;
        self.board_verified = true;
        self.save()
    }

    fn edit_hours(&mut self, index: usize) -> Result<(), CliError> {
        let mut hours = self.hours_defaults();
        match index {
            0 => hours.weekly_target_hours = prompt_weekly_target(hours.weekly_target_hours)?,
            1 => hours.utc_offset_minutes = prompt_utc_offset(hours.utc_offset_minutes)?,
            _ => {
                hours.maximum_concurrent_worklog_requests = number(
                    &settings_copy().concurrency,
                    hours.maximum_concurrent_worklog_requests,
                )?;
            }
        }
        validate_prompted_hours(&hours, self.policy())?;
        if hours.weekly_target_hours > 168 || hours.utc_offset_minutes.abs() > 840 {
            return Err(message(&settings_copy().number_required));
        }
        self.values.hours = Some(hours);
        self.save()
    }

    fn edit_issues(&mut self, index: usize) -> Result<(), CliError> {
        if index == 0 {
            return self.edit_permissions(false);
        }
        let limit = number(&settings_copy().search_limit, self.search_limit())?;
        if self
            .policy()
            .is_some_and(|policy| limit > policy.maximum_allowed_issue_search_results)
        {
            return Err(message(&settings_copy().managed_notice));
        }
        self.values.maximum_issue_search_results = Some(limit);
        self.save()
    }

    fn edit_advanced(&mut self, index: usize) -> Result<(), CliError> {
        let mut limits = self.limits();
        match index {
            0 => limits.timeout_seconds = number(&settings_copy().timeout, limits.timeout_seconds)?,
            1 => limits.page_size = number(&settings_copy().page_size, limits.page_size)?,
            _ => {
                limits.maximum_collection_items = number(
                    &settings_copy().collection_limit,
                    limits.maximum_collection_items,
                )?;
            }
        }
        if self
            .policy()
            .is_some_and(|policy| !jira_request_limits_allowed(limits, policy))
        {
            return Err(message(&settings_copy().managed_notice));
        }
        self.values.request_timeout_seconds = Some(limits.timeout_seconds);
        self.values.page_size = Some(limits.page_size);
        self.values.maximum_collection_items = Some(limits.maximum_collection_items);
        self.save()
    }

    fn edit_permissions(&mut self, hours: bool) -> Result<(), CliError> {
        let options = permission_options(hours, &self.values.capabilities);
        let allowed = self.policy().map(|policy| &policy.mcp_capabilities);
        if options
            .iter()
            .all(|(capability, _, _)| !capability_available(allowed, *capability))
        {
            return show_notice(&settings_copy().no_capabilities);
        }
        let selected = select_capabilities(&settings_copy().permissions, allowed, options.clone())?;
        let mut capabilities = self.values.capabilities.clone();
        for (capability, _, _) in options {
            capabilities.remove(&capability);
        }
        capabilities.extend(selected);
        disable_dependent_jira_capabilities(&mut capabilities);
        self.values.capabilities = capabilities;
        self.save()
    }

    fn hours_defaults(&self) -> JiraHoursConfiguration {
        self.values.hours.clone().unwrap_or_else(|| {
            self.policy()
                .and_then(|policy| profile_jira_hours(policy).ok())
                .unwrap_or_else(default_jira_hours)
        })
    }

    fn limits(&self) -> ProviderRequestLimits {
        let defaults = self
            .policy()
            .map_or_else(default_provider_request_limits, jira_profile_limits);
        ProviderRequestLimits {
            timeout_seconds: self
                .values
                .request_timeout_seconds
                .unwrap_or(defaults.timeout_seconds),
            page_size: self.values.page_size.unwrap_or(defaults.page_size),
            maximum_collection_items: self
                .values
                .maximum_collection_items
                .unwrap_or(defaults.maximum_collection_items),
        }
    }

    fn search_limit(&self) -> usize {
        self.values.maximum_issue_search_results.unwrap_or_else(|| {
            self.policy()
                .map_or(DEFAULT_MAXIMUM_ISSUE_SEARCH_RESULTS, |policy| {
                    policy.maximum_issue_search_results
                })
        })
    }

    fn connection_state(&self) -> &str {
        let copy = settings_copy();
        if self.verified {
            return &copy.verified;
        }
        if self.token().is_ok() {
            &copy.saved
        } else {
            &copy.needs_setup
        }
    }

    fn board_state(&self) -> String {
        let copy = settings_copy();
        self.values.board_id.map_or_else(
            || {
                if self.token().is_ok() {
                    copy.needs_setup.clone()
                } else {
                    copy.blocked.clone()
                }
            },
            |board| {
                format!(
                    "{board} · {}",
                    if self.board_verified {
                        &copy.verified
                    } else {
                        &copy.saved
                    }
                )
            },
        )
    }

    fn capability_state(&self, capability: Capability) -> &str {
        let copy = settings_copy();
        if !self.values.capabilities.contains(&capability) {
            return &copy.disabled;
        }
        if self.values.board_id.is_none() || self.token().is_err() {
            return &copy.needs_setup;
        }
        if capability == Capability::ReadOwnTimeEntries && self.values.hours.is_none() {
            return &copy.needs_setup;
        }
        &copy.ready
    }

    fn managed_state(&self) -> &str {
        if self.policy().is_some() {
            &settings_copy().managed
        } else {
            &settings_copy().ready
        }
    }

    fn save_draft(&self) -> Result<(), CliError> {
        let store =
            SettingsStore::for_current_user().map_err(|error| message(error.to_string()))?;
        let mut document = store
            .load()
            .map_err(|error| message(error.to_string()))?
            .unwrap_or_default();
        update_shared_draft(&mut document, &self.values);
        let revision = document.revision;
        store
            .save(&document, revision)
            .map_err(|error| message(error.to_string()))?;
        Ok(())
    }

    fn save(&self) -> Result<(), CliError> {
        self.save_draft()?;
        let store = ConfigurationStore::for_current_user()?;
        let current = store.load()?;
        if !self.can_apply(current.as_ref()) {
            return show_notice(&settings_copy().pending);
        }
        let configuration = self.configuration(current.as_ref())?;
        let configuration = apply_setup_profile(configuration, self.profile)?;
        store.save(&configuration)?;
        show_notice(&settings_copy().applied)
    }

    fn can_apply(&self, current: Option<&McpConfiguration>) -> bool {
        if self.values.board_id.is_none() || self.token().is_err() {
            return false;
        }
        if self
            .values
            .capabilities
            .contains(&Capability::ReadOwnTimeEntries)
            && self.values.hours.is_none()
        {
            return false;
        }
        (self.verified && self.board_verified)
            || current
                .and_then(|current| current.jira.as_ref())
                .is_some_and(|jira| {
                    self.values.base_url.as_deref() == Some(&jira.base_url)
                        && self.values.email.as_deref() == Some(&jira.email)
                        && self.values.board_id == Some(jira.board_id)
                })
    }

    fn configuration(
        &self,
        current: Option<&McpConfiguration>,
    ) -> Result<McpConfiguration, CliError> {
        let (site, email) = self.coordinates()?;
        let values = JiraSetupValues {
            site: site.into(),
            email: email.into(),
            board_id: self
                .values
                .board_id
                .ok_or_else(|| message(&settings_copy().needs_setup))?,
            capabilities: self.values.capabilities.clone(),
            hours: self.values.hours.clone(),
            limits: self.limits(),
        };
        let mut configuration = configured_jira(values, current, self.policy())?;
        if let Some(jira) = configuration.jira.as_mut() {
            jira.maximum_issue_search_results = self.search_limit();
        }
        configuration.validate()?;
        Ok(configuration)
    }
}

fn update_shared_draft(
    document: &mut worklogger_settings::SettingsDocument,
    values: &JiraSettingsDraft,
) {
    let jira = document.jira.get_or_insert_with(JiraSettings::default);
    jira.base_url.clone_from(&values.base_url);
    jira.email.clone_from(&values.email);
    jira.board_id = values.board_id;
    jira.request_timeout_seconds = values.request_timeout_seconds;
    jira.page_size = values.page_size;
    jira.maximum_collection_items = values.maximum_collection_items;
    jira.maximum_issue_search_results = values.maximum_issue_search_results;
    update_shared_hours(document, values.hours.as_ref());
    update_shared_permissions(document, values.capabilities.clone());
}

fn update_shared_hours(
    document: &mut worklogger_settings::SettingsDocument,
    hours: Option<&JiraHoursConfiguration>,
) {
    let Some(hours) = hours else {
        return;
    };
    let jira = document.jira.get_or_insert_with(JiraSettings::default);
    jira.maximum_concurrent_worklog_requests = Some(hours.maximum_concurrent_worklog_requests);
    let shared = document.hours.get_or_insert_with(HoursSettings::default);
    shared.weekly_target_hours = Some(hours.weekly_target_hours);
    shared.utc_offset_minutes = Some(hours.utc_offset_minutes);
}

fn update_shared_permissions(
    document: &mut worklogger_settings::SettingsDocument,
    capabilities: BTreeSet<Capability>,
) {
    let Some(mcp) = document.mcp.as_mut() else {
        if capabilities.is_empty() {
            return;
        }
        document.mcp = Some(McpSettings::default());
        update_shared_permissions(document, capabilities);
        return;
    };
    let module = mcp.modules.entry(ModuleId::Jira).or_insert(ModuleSettings {
        enabled: !capabilities.is_empty(),
        capabilities: BTreeSet::new(),
    });
    module.enabled = !capabilities.is_empty();
    module.capabilities = capabilities;
}

#[cfg(test)]
fn from_configuration(
    current: Option<&McpConfiguration>,
    profile: Option<&OrganizationProfile>,
) -> JiraSettingsDraft {
    let Some(jira) = current.and_then(|configuration| configuration.jira.as_ref()) else {
        let _ = profile;
        return JiraSettingsDraft::default();
    };
    JiraSettingsDraft {
        base_url: Some(jira.base_url.clone()),
        email: Some(jira.email.clone()),
        board_id: Some(jira.board_id),
        capabilities: current
            .and_then(|configuration| configuration.modules.get(&ModuleId::Jira))
            .filter(|module| module.enabled)
            .map_or_else(BTreeSet::new, |module| module.capabilities.clone()),
        hours: jira.hours.clone(),
        request_timeout_seconds: Some(jira.request_timeout_seconds),
        page_size: Some(jira.page_size),
        maximum_collection_items: Some(jira.maximum_collection_items),
        maximum_issue_search_results: Some(jira.maximum_issue_search_results),
    }
}

fn from_shared(
    document: Option<&worklogger_settings::SettingsDocument>,
    profile: Option<&OrganizationProfile>,
) -> JiraSettingsDraft {
    let Some(jira) = document.and_then(|settings| settings.jira.as_ref()) else {
        let _ = profile;
        return JiraSettingsDraft::default();
    };
    let hours = document
        .and_then(|settings| settings.hours.as_ref())
        .and_then(|hours| {
            Some(JiraHoursConfiguration {
                weekly_target_hours: hours.weekly_target_hours?,
                utc_offset_minutes: hours.utc_offset_minutes?,
                maximum_concurrent_worklog_requests: jira.maximum_concurrent_worklog_requests?,
            })
        });
    let capabilities = document
        .and_then(|settings| settings.mcp.as_ref())
        .and_then(|mcp| mcp.modules.get(&ModuleId::Jira))
        .filter(|module| module.enabled)
        .map_or_else(BTreeSet::new, |module| module.capabilities.clone());
    JiraSettingsDraft {
        base_url: jira.base_url.clone(),
        email: jira.email.clone(),
        board_id: jira.board_id,
        capabilities,
        hours,
        request_timeout_seconds: jira.request_timeout_seconds,
        page_size: jira.page_size,
        maximum_collection_items: jira.maximum_collection_items,
        maximum_issue_search_results: jira.maximum_issue_search_results,
    }
}

fn permission_options(
    hours: bool,
    selected: &BTreeSet<Capability>,
) -> Vec<(Capability, String, bool)> {
    let copy = tui_copy();
    let options = if hours {
        vec![
            (Capability::ReadOwnTimeEntries, &copy.enable_hours),
            (Capability::WriteOwnTimeEntries, &copy.enable_hours_write),
        ]
    } else {
        vec![
            (Capability::ReadJiraIssues, &copy.enable_issue_read),
            (Capability::EditJiraIssues, &copy.enable_issue_edit),
            (Capability::CommentJiraIssues, &copy.enable_issue_comment),
            (
                Capability::TransitionJiraIssues,
                &copy.enable_issue_transition,
            ),
        ]
    };
    options
        .into_iter()
        .map(|(capability, label)| (capability, label.clone(), selected.contains(&capability)))
        .collect()
}

fn page_title(page: Page) -> String {
    let copy = settings_copy();
    let suffix = match page {
        Page::Jira => return copy.jira.clone(),
        Page::Connection => copy.connection.clone(),
        Page::Hours => copy.hours.clone(),
        Page::HoursSettings => format!("{} › {}", copy.hours, copy.settings),
        Page::Issues => copy.issues.clone(),
        Page::Advanced => copy.advanced.clone(),
    };
    format!("{} › {suffix}", copy.jira)
}

fn row(label: &str, value: &str) -> String {
    format!("{label} · {value}")
}

fn number<Number>(label: &str, current: Number) -> Result<Number, CliError>
where
    Number: std::str::FromStr + std::fmt::Display + Default + PartialOrd + Copy,
{
    prompt(label, Some(&current.to_string()))?
        .parse::<Number>()
        .ok()
        .filter(|number| *number > Number::default())
        .ok_or_else(|| message(&settings_copy().number_required))
}

fn show_notice(text: &str) -> Result<(), CliError> {
    show_message(&tui_copy().result_title, &[text.to_owned()])
        .map(|_| ())
        .map_err(|error| terminal_error(&error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importing_existing_configuration_preserves_every_jira_setting() {
        let configuration = configured_jira(
            JiraSetupValues {
                site: "https://example.atlassian.net".into(),
                email: "person@example.com".into(),
                board_id: 42,
                capabilities: BTreeSet::from([Capability::ReadJiraIssues]),
                hours: Some(default_jira_hours()),
                limits: default_provider_request_limits(),
            },
            None,
            None,
        )
        .unwrap();
        let values = from_configuration(Some(&configuration), None);
        let editor = Editor {
            values,
            profile: None,
            verified: false,
            board_verified: false,
        };
        assert_eq!(
            editor.configuration(Some(&configuration)).unwrap(),
            configuration
        );
    }

    #[test]
    fn independent_permission_groups_keep_existing_defaults() {
        let selected = BTreeSet::from([Capability::ReadOwnTimeEntries, Capability::ReadJiraIssues]);
        let hours = permission_options(true, &selected);
        assert_eq!(hours.len(), 2);
        assert!(hours[0].2);
        assert!(!hours[1].2);
        assert!(
            !hours
                .iter()
                .any(|(capability, _, _)| *capability == Capability::ReadJiraIssues)
        );
    }

    #[test]
    fn changing_account_clears_board_and_verification_but_preserves_preferences() {
        let mut editor = Editor {
            values: JiraSettingsDraft {
                board_id: Some(42),
                hours: Some(default_jira_hours()),
                ..JiraSettingsDraft::default()
            },
            profile: None,
            verified: true,
            board_verified: true,
        };
        editor.invalidate_connection();
        assert_eq!(editor.values.board_id, None);
        assert!(!editor.verified);
        assert!(!editor.board_verified);
        assert_eq!(editor.values.hours, Some(default_jira_hours()));
    }
}
