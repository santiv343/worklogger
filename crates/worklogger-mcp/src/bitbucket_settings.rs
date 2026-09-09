//! Independent Bitbucket settings editors. Incomplete values remain a secret-free draft.

use super::settings_draft::{BitbucketSettingsDraft, SettingsDraftStore};
use super::{
    BITBUCKET_CLOUD_API_ORIGIN, BTreeSet, BitbucketConfiguration, BitbucketModuleProfile, CliError,
    ConfigurationStore, CredentialPurpose, CredentialStore, McpConfiguration, ModuleId,
    OrganizationProfile, ProviderRequestLimits, apply_setup_profile, bitbucket_environment_token,
    bitbucket_profile_limits, bitbucket_pull_request_defaults, bitbucket_request_limits_allowed,
    choose, choose_bitbucket_capabilities, choose_bitbucket_workspace, credential_transaction,
    default_provider_request_limits, discover_bitbucket, enabled_bitbucket_module, message, prompt,
    read_bitbucket_setup_token, scoped_bitbucket_repositories, select_bitbucket_repositories,
    settings_copy, show_progress, terminal_error, tui_copy,
};
use settings_copy::settings_copy;

#[derive(Clone, Copy)]
enum Page {
    Bitbucket,
    Connection,
    Scope,
    PullRequests,
    Advanced,
}

struct Editor<'profile> {
    values: BitbucketSettingsDraft,
    profile: Option<&'profile OrganizationProfile>,
    verified: bool,
}

pub(super) async fn run(profile: Option<&OrganizationProfile>) -> Result<(), CliError> {
    let mut editor = Editor::load(profile)?;
    let mut pages = vec![Page::Bitbucket];
    while let Some(page) = pages.last().copied() {
        match editor.step(page, &mut pages).await {
            Ok(()) => {}
            Err(CliError::Cancelled) => {
                pages.pop();
            }
            Err(error) => {
                super::show_message(&tui_copy().error_title, &[error.to_string()])
                    .map_err(|error| terminal_error(&error))?;
            }
        }
    }
    Ok(())
}

impl<'profile> Editor<'profile> {
    fn load(profile: Option<&'profile OrganizationProfile>) -> Result<Self, CliError> {
        let store = ConfigurationStore::for_current_user()?;
        let drafts = SettingsDraftStore::for_configuration(store.path())?;
        let current = store.load()?;
        let values = drafts
            .load()?
            .and_then(|draft| draft.bitbucket)
            .unwrap_or_else(|| from_configuration(current.as_ref()));
        Ok(Self {
            values,
            profile,
            verified: false,
        })
    }

    fn policy(&self) -> Option<&BitbucketModuleProfile> {
        self.profile
            .and_then(|profile| profile.modules.bitbucket.as_ref())
    }

    async fn step(&mut self, page: Page, pages: &mut Vec<Page>) -> Result<(), CliError> {
        let options = self.options(page);
        let index = choose(&page_title(page), &options).map_err(|error| terminal_error(&error))?;
        if index == options.len() - 1 {
            pages.pop();
            return Ok(());
        }
        match page {
            Page::Bitbucket => {
                Self::open_section(index, pages);
                Ok(())
            }
            Page::Connection => self.edit_connection(index),
            Page::Scope => self.edit_scope().await,
            Page::PullRequests => self.edit_pull_requests(index),
            Page::Advanced => self.edit_advanced(index),
        }
    }

    fn options(&self, page: Page) -> Vec<String> {
        let mut options = match page {
            Page::Bitbucket => self.root_options(),
            Page::Connection => self.connection_options(),
            Page::Scope => vec![row(&settings_copy().workspaces, &self.scope_state())],
            Page::PullRequests => self.pull_request_options(),
            Page::Advanced => self.advanced_options(),
        };
        options.push(settings_copy().back.clone());
        options
    }

    fn root_options(&self) -> Vec<String> {
        let copy = settings_copy();
        vec![
            row(&copy.connection, self.connection_state()),
            row(&copy.workspaces, &self.scope_state()),
            row(&copy.pull_requests, self.permissions_state()),
            row(&copy.advanced, self.managed_state()),
        ]
    }

    fn connection_options(&self) -> Vec<String> {
        let copy = settings_copy();
        vec![
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
        ]
    }

    fn pull_request_options(&self) -> Vec<String> {
        let copy = settings_copy();
        vec![
            row(&copy.permissions, self.permissions_state()),
            row(&copy.defaults, &self.defaults_state()),
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

    fn open_section(index: usize, pages: &mut Vec<Page>) {
        match index {
            0 => pages.push(Page::Connection),
            1 => pages.push(Page::Scope),
            2 => pages.push(Page::PullRequests),
            _ => pages.push(Page::Advanced),
        }
    }

    fn edit_connection(&mut self, index: usize) -> Result<(), CliError> {
        if index == 0 {
            self.edit_email()?;
        } else {
            self.replace_token()?;
        }
        self.save_draft()?;
        show_notice(&settings_copy().connection_changed);
        Ok(())
    }

    fn edit_email(&mut self) -> Result<(), CliError> {
        let email = prompt(&settings_copy().email, self.values.email.as_deref())?;
        if !email.contains('@') {
            return Err(message(&settings_copy().connection_required));
        }
        if self.values.email.as_ref() != Some(&email) {
            self.values.email = Some(email);
            self.values.workspaces.clear();
            self.verified = false;
        }
        Ok(())
    }

    fn replace_token(&mut self) -> Result<(), CliError> {
        let email = self.email()?;
        let token = read_bitbucket_setup_token()?;
        let _guard = credential_transaction()?;
        CredentialStore::for_purpose(CredentialPurpose::Mcp)
            .and_then(|store| store.save_api_token(BITBUCKET_CLOUD_API_ORIGIN, email, &token))
            .map_err(|error| message(error.to_string()))?;
        self.verified = false;
        Ok(())
    }

    async fn edit_scope(&mut self) -> Result<(), CliError> {
        let email = self.email()?;
        let token = self.token()?;
        let workspace = choose_bitbucket_workspace(self.policy())?;
        show_progress(&tui_copy().working_title, &tui_copy().validating_account)
            .map_err(|error| terminal_error(&error))?;
        let (_, repositories) =
            discover_bitbucket(email, &token, &workspace, self.limits()).await?;
        let repositories = scoped_bitbucket_repositories(self.policy(), &workspace, repositories);
        let selected = select_bitbucket_repositories(self.policy(), &repositories)?;
        self.values.workspaces.insert(workspace, selected);
        self.verified = true;
        self.save()
    }

    fn edit_pull_requests(&mut self, index: usize) -> Result<(), CliError> {
        if index == 0 {
            self.values.capabilities = choose_bitbucket_capabilities(
                self.policy().map(|policy| &policy.mcp_capabilities),
            )?;
        } else {
            self.values.pull_request_defaults = bitbucket_pull_request_defaults()?;
        }
        self.save()
    }

    fn edit_advanced(&mut self, index: usize) -> Result<(), CliError> {
        let mut limits = self.limits();
        match index {
            0 => {
                limits.timeout_seconds = number(&settings_copy().timeout, &limits.timeout_seconds)?;
            }
            1 => limits.page_size = number(&settings_copy().page_size, &limits.page_size)?,
            _ => {
                limits.maximum_collection_items = number(
                    &settings_copy().collection_limit,
                    &limits.maximum_collection_items,
                )?;
            }
        }
        if self
            .policy()
            .is_some_and(|policy| !bitbucket_request_limits_allowed(limits, policy))
        {
            return Err(message(&settings_copy().managed_notice));
        }
        self.values.request_timeout_seconds = Some(limits.timeout_seconds);
        self.values.page_size = Some(limits.page_size);
        self.values.maximum_collection_items = Some(limits.maximum_collection_items);
        self.save()
    }

    fn email(&self) -> Result<&str, CliError> {
        self.values
            .email
            .as_deref()
            .ok_or_else(|| message(&settings_copy().connection_required))
    }

    fn token(&self) -> Result<String, CliError> {
        if let Some(token) = bitbucket_environment_token() {
            return Ok(token);
        }
        let email = self.email()?;
        CredentialStore::for_purpose(CredentialPurpose::Mcp)
            .map_err(|error| message(error.to_string()))?
            .load_api_token(BITBUCKET_CLOUD_API_ORIGIN, email)
            .map_err(|error| message(error.to_string()))?
            .ok_or(CliError::MissingBitbucketToken)
    }

    fn limits(&self) -> ProviderRequestLimits {
        let defaults = self
            .policy()
            .map_or_else(default_provider_request_limits, bitbucket_profile_limits);
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

    fn scope_state(&self) -> String {
        let copy = settings_copy();
        if self.values.workspaces.is_empty() {
            return if self.token().is_ok() {
                copy.needs_setup.clone()
            } else {
                copy.blocked.clone()
            };
        }
        let repositories = self
            .values
            .workspaces
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        format!(
            "{} workspaces · {repositories} repositories",
            self.values.workspaces.len()
        )
    }

    fn permissions_state(&self) -> &str {
        let copy = settings_copy();
        if self.values.capabilities.is_empty() {
            return &copy.disabled;
        }
        if self.values.workspaces.is_empty() || self.token().is_err() {
            return &copy.needs_setup;
        }
        &copy.ready
    }

    fn defaults_state(&self) -> String {
        let defaults = &self.values.pull_request_defaults;
        format!(
            "{} reviewers · {}",
            defaults.reviewer_account_ids.len(),
            if defaults.close_source_branch {
                "close branch"
            } else {
                "keep branch"
            }
        )
    }

    fn managed_state(&self) -> &str {
        if self.policy().is_some() {
            &settings_copy().managed
        } else {
            &settings_copy().ready
        }
    }

    fn save_draft(&self) -> Result<(), CliError> {
        let store = ConfigurationStore::for_current_user()?;
        let drafts = SettingsDraftStore::for_configuration(store.path())?;
        let mut draft = drafts.load()?.unwrap_or_default();
        draft.bitbucket = Some(self.values.clone());
        drafts.save(&draft)?;
        Ok(())
    }

    fn save(&self) -> Result<(), CliError> {
        self.save_draft()?;
        if self.values.workspaces.is_empty() || self.token().is_err() {
            show_notice(&settings_copy().pending);
            return Ok(());
        }
        let store = ConfigurationStore::for_current_user()?;
        let current = store.load()?;
        let configuration =
            apply_setup_profile(self.configuration(current.as_ref())?, self.profile)?;
        store.save(&configuration)?;
        Self::clear_draft(&store)?;
        show_notice(&settings_copy().applied);
        Ok(())
    }

    fn configuration(
        &self,
        current: Option<&McpConfiguration>,
    ) -> Result<McpConfiguration, CliError> {
        let bitbucket = BitbucketConfiguration {
            email: self.email()?.to_owned(),
            workspaces: self.values.workspaces.clone(),
            request_timeout_seconds: self.limits().timeout_seconds,
            page_size: self.limits().page_size,
            maximum_collection_items: self.limits().maximum_collection_items,
            pull_request_defaults: self.values.pull_request_defaults.clone(),
        };
        let modules = enabled_bitbucket_module(self.values.capabilities.clone(), current);
        match current.and_then(|configuration| configuration.jira.clone()) {
            Some(jira) => McpConfiguration::new_with_bitbucket(jira, bitbucket, modules),
            None => McpConfiguration::new_bitbucket(bitbucket, modules),
        }
        .map_err(CliError::from)
    }

    fn clear_draft(store: &ConfigurationStore) -> Result<(), CliError> {
        let drafts = SettingsDraftStore::for_configuration(store.path())?;
        let mut draft = drafts.load()?.unwrap_or_default();
        draft.bitbucket = None;
        if draft.jira.is_none() {
            drafts.clear()?;
        } else {
            drafts.save(&draft)?;
        }
        Ok(())
    }
}

fn from_configuration(current: Option<&McpConfiguration>) -> BitbucketSettingsDraft {
    let Some(bitbucket) = current.and_then(|configuration| configuration.bitbucket.as_ref()) else {
        return BitbucketSettingsDraft::default();
    };
    BitbucketSettingsDraft {
        email: Some(bitbucket.email.clone()),
        workspaces: bitbucket.workspaces.clone(),
        capabilities: current
            .and_then(|configuration| configuration.modules.get(&ModuleId::Bitbucket))
            .filter(|module| module.enabled)
            .map_or_else(BTreeSet::new, |module| module.capabilities.clone()),
        pull_request_defaults: bitbucket.pull_request_defaults.clone(),
        request_timeout_seconds: Some(bitbucket.request_timeout_seconds),
        page_size: Some(bitbucket.page_size),
        maximum_collection_items: Some(bitbucket.maximum_collection_items),
    }
}

fn page_title(page: Page) -> String {
    let copy = settings_copy();
    match page {
        Page::Bitbucket => copy.bitbucket.clone(),
        Page::Connection => format!("{} › {}", copy.bitbucket, copy.connection),
        Page::Scope => format!("{} › {}", copy.bitbucket, copy.workspaces),
        Page::PullRequests => format!("{} › {}", copy.bitbucket, copy.pull_requests),
        Page::Advanced => format!("{} › {}", copy.bitbucket, copy.advanced),
    }
}

fn row(label: &str, value: &str) -> String {
    format!("{label} · {value}")
}

fn number<Value>(label: &str, current: &impl ToString) -> Result<Value, CliError>
where
    Value: std::str::FromStr,
{
    let current = current.to_string();
    prompt(label, Some(&current))?
        .parse()
        .map_err(|_| message(&settings_copy().number_required))
}

fn show_notice(value: &str) {
    super::terminal_notice(value.to_owned());
}
