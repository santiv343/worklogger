use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod storage;

pub use storage::{OrganizationProfileStore, ProfileStorageError};

pub const PROFILE_SCHEMA_VERSION: u16 = 2;
const LEGACY_PROFILE_SCHEMA_VERSION: u16 = 1;
const REQUIRED_CHART_COLORS: usize = 6;
const HEX_COLOR_LENGTH: usize = 7;
const MAXIMUM_TEXT_LENGTH: usize = 200;
const MAXIMUM_SITES: usize = 50;
const MAXIMUM_PALETTE_COLORS: usize = 32;
const MAXIMUM_SCOPED_WORKSPACES: usize = 100;
const MAXIMUM_SCOPED_REPOSITORIES: usize = 1_000;
const ABSOLUTE_REQUEST_TIMEOUT_SECONDS: u64 = 300;
const ABSOLUTE_PAGE_SIZE: u16 = 100;
const ABSOLUTE_COLLECTION_ITEMS: usize = 250_000;
const ABSOLUTE_SEARCH_RESULTS: usize = 5_000;
const ABSOLUTE_CONCURRENT_REQUESTS: usize = 64;
const ABSOLUTE_CUSTOM_RANGE_DAYS: u16 = 366;
const ABSOLUTE_UTC_OFFSET_MINUTES: i16 = 840;
const MINIMUM_TEXT_CONTRAST_RATIO: f64 = 4.5;
const RGB_CHANNEL_MAXIMUM: f64 = 255.0;
const SRGB_LINEAR_THRESHOLD: f64 = 0.039_28;
const SRGB_LINEAR_OFFSET: f64 = 0.055;
const SRGB_LINEAR_DIVISOR: f64 = 1.055;
const SRGB_LINEAR_EXPONENT: f64 = 2.4;
const SRGB_DARK_DIVISOR: f64 = 12.92;
const LUMINANCE_RED_WEIGHT: f64 = 0.212_6;
const LUMINANCE_GREEN_WEIGHT: f64 = 0.715_2;
const LUMINANCE_BLUE_WEIGHT: f64 = 0.072_2;
const CONTRAST_LUMINANCE_OFFSET: f64 = 0.05;
const WHITE_HEX_COLOR: &str = "#FFFFFF";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Capability {
    #[serde(rename = "jira.issue.read")]
    ReadJiraIssues,
    #[serde(rename = "jira.issue.edit")]
    EditJiraIssues,
    #[serde(rename = "jira.issue.comment")]
    CommentJiraIssues,
    #[serde(rename = "jira.issue.transition")]
    TransitionJiraIssues,
    #[serde(rename = "jira.hours.read.self", alias = "time-entry.read.self")]
    ReadOwnTimeEntries,
    #[serde(rename = "jira.hours.write.self")]
    WriteOwnTimeEntries,
    #[serde(rename = "bitbucket.pr.read")]
    ReadBitbucketPullRequests,
    #[serde(rename = "bitbucket.pr.create")]
    CreateBitbucketPullRequests,
    #[serde(rename = "bitbucket.pr.edit")]
    EditBitbucketPullRequests,
    #[serde(rename = "bitbucket.pr.comment")]
    CommentBitbucketPullRequests,
    #[serde(rename = "bitbucket.pr.review")]
    ReviewBitbucketPullRequests,
    #[serde(rename = "bitbucket.pr.merge")]
    MergeBitbucketPullRequests,
    #[serde(rename = "bitbucket.pr.decline")]
    DeclineBitbucketPullRequests,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrationModuleId {
    Jira,
    Bitbucket,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderScopeMode {
    Unrestricted,
    Restricted,
}

pub const JIRA_CAPABILITIES: [Capability; 6] = [
    Capability::ReadJiraIssues,
    Capability::EditJiraIssues,
    Capability::CommentJiraIssues,
    Capability::TransitionJiraIssues,
    Capability::ReadOwnTimeEntries,
    Capability::WriteOwnTimeEntries,
];

pub const BITBUCKET_CAPABILITIES: [Capability; 7] = [
    Capability::ReadBitbucketPullRequests,
    Capability::CreateBitbucketPullRequests,
    Capability::EditBitbucketPullRequests,
    Capability::CommentBitbucketPullRequests,
    Capability::ReviewBitbucketPullRequests,
    Capability::MergeBitbucketPullRequests,
    Capability::DeclineBitbucketPullRequests,
];

impl Capability {
    #[must_use]
    pub const fn module(self) -> IntegrationModuleId {
        match self {
            Self::ReadJiraIssues
            | Self::EditJiraIssues
            | Self::CommentJiraIssues
            | Self::TransitionJiraIssues
            | Self::ReadOwnTimeEntries
            | Self::WriteOwnTimeEntries => IntegrationModuleId::Jira,
            Self::ReadBitbucketPullRequests
            | Self::CreateBitbucketPullRequests
            | Self::EditBitbucketPullRequests
            | Self::CommentBitbucketPullRequests
            | Self::ReviewBitbucketPullRequests
            | Self::MergeBitbucketPullRequests
            | Self::DeclineBitbucketPullRequests => IntegrationModuleId::Bitbucket,
        }
    }

    #[must_use]
    pub const fn required_read(self) -> Option<Self> {
        match self {
            Self::EditJiraIssues | Self::CommentJiraIssues | Self::TransitionJiraIssues => {
                Some(Self::ReadJiraIssues)
            }
            Self::WriteOwnTimeEntries => Some(Self::ReadOwnTimeEntries),
            Self::CreateBitbucketPullRequests
            | Self::EditBitbucketPullRequests
            | Self::CommentBitbucketPullRequests
            | Self::ReviewBitbucketPullRequests
            | Self::MergeBitbucketPullRequests
            | Self::DeclineBitbucketPullRequests => Some(Self::ReadBitbucketPullRequests),
            Self::ReadJiraIssues | Self::ReadOwnTimeEntries | Self::ReadBitbucketPullRequests => {
                None
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrganizationProfile {
    pub schema_version: u16,
    pub branding: BrandingProfile,
    pub modules: ModuleProfiles,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleProfiles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraModuleProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitbucket: Option<BitbucketModuleProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reports: Option<ReportsModuleProfile>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrandingProfile {
    pub company_name: String,
    pub logo_url: Option<String>,
    pub primary_color: String,
    pub text_color: String,
    pub muted_color: String,
    pub surface_color: String,
    pub border_color: String,
    pub chart_palette: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraModuleProfile {
    pub scope_mode: ProviderScopeMode,
    pub sites: Vec<JiraSiteProfile>,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    pub maximum_issue_search_results: usize,
    pub maximum_concurrent_worklog_requests: usize,
    pub maximum_allowed_request_timeout_seconds: u64,
    pub maximum_allowed_page_size: u16,
    pub maximum_allowed_collection_items: usize,
    pub maximum_allowed_issue_search_results: usize,
    pub maximum_allowed_concurrent_worklog_requests: usize,
    pub mcp_capabilities: BTreeSet<Capability>,
    pub hours: HoursProfile,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JiraSiteProfile {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_board_ids: Option<BTreeSet<u64>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HoursProfile {
    pub minimum_weekly_target_hours: u32,
    pub maximum_weekly_target_hours: u32,
    pub suggested_weekly_target_hours: u32,
    pub maximum_daily_hours: u8,
    pub default_worklog_start_hour: u8,
    pub default_worklog_start_minute: u8,
    pub default_duration_minutes: u32,
    pub maximum_custom_range_days: u16,
    pub suggested_utc_offset_minutes: i16,
    pub utc_offset_options: Vec<UtcOffsetOption>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UtcOffsetOption {
    pub minutes: i16,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BitbucketModuleProfile {
    pub scope_mode: ProviderScopeMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<BTreeMap<String, BTreeSet<String>>>,
    pub request_timeout_seconds: u64,
    pub page_size: u16,
    pub maximum_collection_items: usize,
    pub maximum_allowed_request_timeout_seconds: u64,
    pub maximum_allowed_page_size: u16,
    pub maximum_allowed_collection_items: usize,
    pub mcp_capabilities: BTreeSet<Capability>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportsModuleProfile {
    pub pdf_export_enabled: bool,
    #[serde(default = "default_team_report_access_policy")]
    pub team_report_access_policy: TeamReportAccessPolicy,
    pub maximum_task_slices: usize,
    pub maximum_trend_labels: usize,
    pub maximum_team_chart_members: usize,
    pub table_page_size: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TeamReportAccessPolicy {
    ProjectAdmin,
    BrowseProject,
}

impl TeamReportAccessPolicy {
    #[must_use]
    pub const fn allows(self, browse_projects: bool, administer_projects: bool) -> bool {
        match self {
            Self::ProjectAdmin => administer_projects,
            Self::BrowseProject => browse_projects,
        }
    }
}

const fn default_team_report_access_policy() -> TeamReportAccessPolicy {
    TeamReportAccessPolicy::ProjectAdmin
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("the profile JSON is invalid: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("no se pudo serializar el perfil: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("schemaVersion no es compatible")]
    UnsupportedSchema,
    #[error("branding is invalid")]
    InvalidBranding,
    #[error("the Jira module is invalid: {0}")]
    InvalidJira(&'static str),
    #[error("the Bitbucket module is invalid: {0}")]
    InvalidBitbucket(&'static str),
    #[error("the Reports module is invalid")]
    InvalidReports,
    #[error("the profile must configure at least one module")]
    EmptyModules,
    #[error("una capacidad corresponde a {actual:?}, no a {expected:?}")]
    CapabilityModuleMismatch {
        expected: IntegrationModuleId,
        actual: IntegrationModuleId,
    },
    #[error("la capacidad {capability:?} requiere {required:?}")]
    MissingRequiredCapability {
        capability: Capability,
        required: Capability,
    },
}

impl OrganizationProfile {
    /// Parses and validates a current or legacy organization profile.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid JSON, unsupported schemas or unsafe values.
    pub fn from_json(contents: &str) -> Result<Self, ProfileError> {
        let header: SchemaHeader = serde_json::from_str(contents).map_err(ProfileError::Decode)?;
        let profile = match header.schema_version {
            PROFILE_SCHEMA_VERSION => {
                serde_json::from_str(contents).map_err(ProfileError::Decode)?
            }
            LEGACY_PROFILE_SCHEMA_VERSION => legacy_profile(contents)?.into_current(),
            _ => return Err(ProfileError::UnsupportedSchema),
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Serializes the canonical modular representation.
    ///
    /// # Errors
    ///
    /// Returns an error when serialization fails.
    pub fn to_pretty_json(&self) -> Result<String, ProfileError> {
        let mut document = serde_json::to_string_pretty(self).map_err(ProfileError::Encode)?;
        document.push('\n');
        Ok(document)
    }

    /// Validates module scopes, limits and capability ownership.
    ///
    /// # Errors
    ///
    /// Returns an error when the profile cannot be applied safely.
    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ProfileError::UnsupportedSchema);
        }
        validate_branding(&self.branding)?;
        validate_non_empty_modules(&self.modules)?;
        validate_optional_modules(&self.modules)
    }

    #[must_use]
    pub fn allows(&self, capability: Capability) -> bool {
        match capability.module() {
            IntegrationModuleId::Jira => self
                .modules
                .jira
                .as_ref()
                .is_some_and(|module| module.mcp_capabilities.contains(&capability)),
            IntegrationModuleId::Bitbucket => self
                .modules
                .bitbucket
                .as_ref()
                .is_some_and(|module| module.mcp_capabilities.contains(&capability)),
        }
    }

    #[must_use]
    pub fn allows_jira_board(&self, site_url: &str, board_id: u64) -> bool {
        let Some(jira) = self.modules.jira.as_ref() else {
            return false;
        };
        jira.allows_board(site_url, board_id)
    }

    #[must_use]
    pub fn allows_bitbucket_repository(&self, workspace: &str, repository: &str) -> bool {
        let Some(bitbucket) = self.modules.bitbucket.as_ref() else {
            return false;
        };
        bitbucket.allows_repository(workspace, repository)
    }
}

impl JiraModuleProfile {
    #[must_use]
    pub fn allows_site(&self, site_url: &str) -> bool {
        if self.scope_mode == ProviderScopeMode::Unrestricted {
            return true;
        }
        self.sites.iter().any(|site| same_site(&site.url, site_url))
    }

    #[must_use]
    pub fn allows_board(&self, site_url: &str, board_id: u64) -> bool {
        if self.scope_mode == ProviderScopeMode::Unrestricted {
            return true;
        }
        self.sites
            .iter()
            .find(|site| same_site(&site.url, site_url))
            .is_some_and(|site| {
                site.allowed_board_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&board_id))
            })
    }
}

impl HoursProfile {
    #[must_use]
    pub fn allows_weekly_target(&self, weekly_target_hours: u32) -> bool {
        (self.minimum_weekly_target_hours..=self.maximum_weekly_target_hours)
            .contains(&weekly_target_hours)
    }

    #[must_use]
    pub fn allows_utc_offset(&self, utc_offset_minutes: i16) -> bool {
        self.utc_offset_options
            .iter()
            .any(|option| option.minutes == utc_offset_minutes)
    }
}

impl BitbucketModuleProfile {
    #[must_use]
    pub fn allows_repository(&self, workspace: &str, repository: &str) -> bool {
        if self.scope_mode == ProviderScopeMode::Unrestricted {
            return true;
        }
        let Some(workspaces) = self.workspaces.as_ref() else {
            return false;
        };
        workspaces
            .get(workspace)
            .is_some_and(|repositories| repositories.contains(repository))
    }
}

fn same_site(first: &str, second: &str) -> bool {
    first
        .trim_end_matches('/')
        .eq_ignore_ascii_case(second.trim_end_matches('/'))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaHeader {
    schema_version: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyProfile {
    schema_version: u16,
    branding: BrandingProfile,
    jira: LegacyJiraProfile,
    hours: HoursProfile,
    reports: ReportsModuleProfile,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyJiraProfile {
    sites: Vec<LegacyJiraSiteProfile>,
    request_timeout_seconds: u64,
    page_size: u16,
    maximum_collection_items: usize,
    maximum_issue_search_results: usize,
    maximum_concurrent_worklog_requests: usize,
    maximum_allowed_request_timeout_seconds: u64,
    maximum_allowed_page_size: u16,
    maximum_allowed_collection_items: usize,
    maximum_allowed_issue_search_results: usize,
    maximum_allowed_concurrent_worklog_requests: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyJiraSiteProfile {
    name: String,
    url: String,
}

impl LegacyProfile {
    fn into_current(self) -> OrganizationProfile {
        let jira = self.jira.into_current(self.hours);
        OrganizationProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            branding: self.branding,
            modules: ModuleProfiles {
                jira: Some(jira),
                bitbucket: None,
                reports: Some(self.reports),
            },
        }
    }
}

impl LegacyJiraProfile {
    fn into_current(self, hours: HoursProfile) -> JiraModuleProfile {
        let scope_mode = legacy_jira_scope_mode(&self.sites);
        JiraModuleProfile {
            scope_mode,
            sites: self.sites.into_iter().map(legacy_site).collect(),
            request_timeout_seconds: self.request_timeout_seconds,
            page_size: self.page_size,
            maximum_collection_items: self.maximum_collection_items,
            maximum_issue_search_results: self.maximum_issue_search_results,
            maximum_concurrent_worklog_requests: self.maximum_concurrent_worklog_requests,
            maximum_allowed_request_timeout_seconds: self.maximum_allowed_request_timeout_seconds,
            maximum_allowed_page_size: self.maximum_allowed_page_size,
            maximum_allowed_collection_items: self.maximum_allowed_collection_items,
            maximum_allowed_issue_search_results: self.maximum_allowed_issue_search_results,
            maximum_allowed_concurrent_worklog_requests: self
                .maximum_allowed_concurrent_worklog_requests,
            mcp_capabilities: BTreeSet::from(JIRA_CAPABILITIES),
            hours,
        }
    }
}

fn legacy_jira_scope_mode(sites: &[LegacyJiraSiteProfile]) -> ProviderScopeMode {
    if sites.is_empty() {
        ProviderScopeMode::Unrestricted
    } else {
        ProviderScopeMode::Restricted
    }
}

fn legacy_profile(contents: &str) -> Result<LegacyProfile, ProfileError> {
    let profile: LegacyProfile = serde_json::from_str(contents).map_err(ProfileError::Decode)?;
    if profile.schema_version != LEGACY_PROFILE_SCHEMA_VERSION {
        return Err(ProfileError::UnsupportedSchema);
    }
    Ok(profile)
}

fn legacy_site(site: LegacyJiraSiteProfile) -> JiraSiteProfile {
    JiraSiteProfile {
        name: site.name,
        url: site.url,
        allowed_board_ids: None,
    }
}

fn validate_non_empty_modules(modules: &ModuleProfiles) -> Result<(), ProfileError> {
    if modules.jira.is_none() && modules.bitbucket.is_none() && modules.reports.is_none() {
        return Err(ProfileError::EmptyModules);
    }
    Ok(())
}

fn validate_optional_modules(modules: &ModuleProfiles) -> Result<(), ProfileError> {
    if let Some(jira) = modules.jira.as_ref() {
        validate_jira(jira)?;
    }
    if let Some(bitbucket) = modules.bitbucket.as_ref() {
        validate_bitbucket(bitbucket)?;
    }
    if let Some(reports) = modules.reports.as_ref() {
        validate_reports(reports)?;
    }
    Ok(())
}

fn validate_branding(branding: &BrandingProfile) -> Result<(), ProfileError> {
    if invalid_branding_text(branding) || !valid_branding_colors(branding) {
        return Err(ProfileError::InvalidBranding);
    }
    Ok(())
}

fn invalid_branding_text(branding: &BrandingProfile) -> bool {
    let invalid_logo = branding.logo_url.as_deref().is_some_and(invalid_https_url);
    branding.company_name.trim().is_empty()
        || branding.company_name.len() > MAXIMUM_TEXT_LENGTH
        || invalid_logo
}

fn valid_branding_colors(branding: &BrandingProfile) -> bool {
    let colors = [
        &branding.primary_color,
        &branding.text_color,
        &branding.muted_color,
        &branding.surface_color,
        &branding.border_color,
    ];
    colors.into_iter().all(|color| is_hex_color(color))
        && valid_palette(branding)
        && valid_branding_contrast(branding)
}

fn valid_palette(branding: &BrandingProfile) -> bool {
    (REQUIRED_CHART_COLORS..=MAXIMUM_PALETTE_COLORS).contains(&branding.chart_palette.len())
        && branding
            .chart_palette
            .iter()
            .all(|color| is_hex_color(color))
}

fn valid_branding_contrast(branding: &BrandingProfile) -> bool {
    sufficient_contrast(&branding.text_color, &branding.surface_color)
        && sufficient_contrast(&branding.muted_color, &branding.surface_color)
        && sufficient_contrast(&branding.primary_color, &branding.surface_color)
        && sufficient_contrast(&branding.primary_color, WHITE_HEX_COLOR)
}

fn sufficient_contrast(foreground: &str, background: &str) -> bool {
    contrast_ratio(foreground, background).is_some_and(|ratio| ratio >= MINIMUM_TEXT_CONTRAST_RATIO)
}

fn contrast_ratio(first: &str, second: &str) -> Option<f64> {
    let first_luminance = relative_luminance(first)?;
    let second_luminance = relative_luminance(second)?;
    let lighter = first_luminance.max(second_luminance);
    let darker = first_luminance.min(second_luminance);
    Some((lighter + CONTRAST_LUMINANCE_OFFSET) / (darker + CONTRAST_LUMINANCE_OFFSET))
}

fn relative_luminance(value: &str) -> Option<f64> {
    let red = linear_channel(hex_channel(value, 1)?);
    let green = linear_channel(hex_channel(value, 3)?);
    let blue = linear_channel(hex_channel(value, 5)?);
    Some(red * LUMINANCE_RED_WEIGHT + green * LUMINANCE_GREEN_WEIGHT + blue * LUMINANCE_BLUE_WEIGHT)
}

fn hex_channel(value: &str, start: usize) -> Option<u8> {
    value
        .get(start..start.saturating_add(2))
        .and_then(|pair| u8::from_str_radix(pair, 16).ok())
}

fn linear_channel(value: u8) -> f64 {
    let channel = f64::from(value) / RGB_CHANNEL_MAXIMUM;
    if channel <= SRGB_LINEAR_THRESHOLD {
        return channel / SRGB_DARK_DIVISOR;
    }
    ((channel + SRGB_LINEAR_OFFSET) / SRGB_LINEAR_DIVISOR).powf(SRGB_LINEAR_EXPONENT)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == HEX_COLOR_LENGTH
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn invalid_https_url(value: &str) -> bool {
    let Some(remainder) = value.strip_prefix("https://") else {
        return true;
    };
    let authority = remainder.split('/').next().unwrap_or(remainder);
    authority.is_empty()
        || authority.contains('@')
        || value.contains('?')
        || value.contains('#')
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
}

fn validate_jira(jira: &JiraModuleProfile) -> Result<(), ProfileError> {
    if !valid_jira_scope(jira) {
        return Err(ProfileError::InvalidJira("sites"));
    }
    if !valid_jira_limits(jira) {
        return Err(ProfileError::InvalidJira("limits"));
    }
    validate_hours(&jira.hours)?;
    validate_capabilities(&jira.mcp_capabilities, IntegrationModuleId::Jira)
}

fn valid_jira_scope(jira: &JiraModuleProfile) -> bool {
    let valid_count = jira.sites.len() <= MAXIMUM_SITES;
    let entries_valid = !jira.sites.iter().any(invalid_site);
    let mode_valid = match jira.scope_mode {
        ProviderScopeMode::Unrestricted => jira.sites.is_empty(),
        ProviderScopeMode::Restricted => !jira.sites.is_empty(),
    };
    valid_count && entries_valid && mode_valid
}

fn invalid_site(site: &JiraSiteProfile) -> bool {
    let invalid_board_scope = site
        .allowed_board_ids
        .as_ref()
        .is_some_and(|ids| ids.is_empty() || ids.contains(&0));
    site.name.trim().is_empty()
        || site.name.len() > MAXIMUM_TEXT_LENGTH
        || site.url.len() > MAXIMUM_TEXT_LENGTH
        || invalid_https_url(&site.url)
        || invalid_board_scope
}

fn valid_jira_limits(jira: &JiraModuleProfile) -> bool {
    jira.request_timeout_seconds > 0
        && jira.page_size > 0
        && jira.maximum_collection_items > 0
        && jira.maximum_issue_search_results > 0
        && jira.maximum_concurrent_worklog_requests > 0
        && jira.request_timeout_seconds <= jira.maximum_allowed_request_timeout_seconds
        && jira.page_size <= jira.maximum_allowed_page_size
        && jira.maximum_collection_items <= jira.maximum_allowed_collection_items
        && jira.maximum_issue_search_results <= jira.maximum_allowed_issue_search_results
        && jira.maximum_concurrent_worklog_requests
            <= jira.maximum_allowed_concurrent_worklog_requests
        && valid_absolute_jira_limits(jira)
}

fn valid_absolute_jira_limits(jira: &JiraModuleProfile) -> bool {
    jira.maximum_allowed_request_timeout_seconds <= ABSOLUTE_REQUEST_TIMEOUT_SECONDS
        && jira.maximum_allowed_page_size <= ABSOLUTE_PAGE_SIZE
        && jira.maximum_allowed_collection_items <= ABSOLUTE_COLLECTION_ITEMS
        && jira.maximum_allowed_issue_search_results <= ABSOLUTE_SEARCH_RESULTS
        && jira.maximum_allowed_concurrent_worklog_requests <= ABSOLUTE_CONCURRENT_REQUESTS
}

fn validate_hours(hours: &HoursProfile) -> Result<(), ProfileError> {
    if !valid_hours_limits(hours) || !valid_utc_offsets(hours) {
        return Err(ProfileError::InvalidJira("hours"));
    }
    Ok(())
}

fn valid_hours_limits(hours: &HoursProfile) -> bool {
    hours.minimum_weekly_target_hours > 0
        && hours.maximum_weekly_target_hours >= hours.minimum_weekly_target_hours
        && (hours.minimum_weekly_target_hours..=hours.maximum_weekly_target_hours)
            .contains(&hours.suggested_weekly_target_hours)
        && hours.maximum_weekly_target_hours <= 168
        && (1..=24).contains(&hours.maximum_daily_hours)
        && hours.default_worklog_start_hour <= 23
        && hours.default_worklog_start_minute <= 59
        && (1..=1_440).contains(&hours.default_duration_minutes)
        && (1..=ABSOLUTE_CUSTOM_RANGE_DAYS).contains(&hours.maximum_custom_range_days)
}

fn valid_utc_offsets(hours: &HoursProfile) -> bool {
    !hours.utc_offset_options.is_empty()
        && hours.utc_offset_options.iter().all(valid_utc_offset)
        && hours
            .utc_offset_options
            .iter()
            .any(|option| option.minutes == hours.suggested_utc_offset_minutes)
}

fn valid_utc_offset(option: &UtcOffsetOption) -> bool {
    option.minutes.abs() <= ABSOLUTE_UTC_OFFSET_MINUTES
        && !option.label.trim().is_empty()
        && option.label.len() <= MAXIMUM_TEXT_LENGTH
}

fn validate_bitbucket(bitbucket: &BitbucketModuleProfile) -> Result<(), ProfileError> {
    if !valid_bitbucket_limits(bitbucket) {
        return Err(ProfileError::InvalidBitbucket("limits"));
    }
    validate_workspace_scope(bitbucket.scope_mode, bitbucket.workspaces.as_ref())?;
    validate_capabilities(&bitbucket.mcp_capabilities, IntegrationModuleId::Bitbucket)
}

fn valid_bitbucket_limits(bitbucket: &BitbucketModuleProfile) -> bool {
    valid_provider_limits(
        bitbucket.maximum_allowed_request_timeout_seconds,
        bitbucket.maximum_allowed_page_size,
        bitbucket.maximum_allowed_collection_items,
    ) && bitbucket.request_timeout_seconds <= bitbucket.maximum_allowed_request_timeout_seconds
        && bitbucket.page_size <= bitbucket.maximum_allowed_page_size
        && bitbucket.maximum_collection_items <= bitbucket.maximum_allowed_collection_items
}

fn valid_provider_limits(timeout_seconds: u64, page_size: u16, maximum_items: usize) -> bool {
    (1..=ABSOLUTE_REQUEST_TIMEOUT_SECONDS).contains(&timeout_seconds)
        && (1..=ABSOLUTE_PAGE_SIZE).contains(&page_size)
        && (1..=ABSOLUTE_COLLECTION_ITEMS).contains(&maximum_items)
}

fn validate_workspace_scope(
    scope_mode: ProviderScopeMode,
    workspaces: Option<&BTreeMap<String, BTreeSet<String>>>,
) -> Result<(), ProfileError> {
    match (scope_mode, workspaces) {
        (ProviderScopeMode::Unrestricted, None) => Ok(()),
        (ProviderScopeMode::Restricted, Some(values)) if valid_workspaces(values) => Ok(()),
        _ => Err(ProfileError::InvalidBitbucket("workspaces")),
    }
}

fn valid_workspaces(workspaces: &BTreeMap<String, BTreeSet<String>>) -> bool {
    if !valid_workspace_count(workspaces) {
        return false;
    }
    !workspaces.iter().any(invalid_workspace)
}

fn valid_workspace_count(workspaces: &BTreeMap<String, BTreeSet<String>>) -> bool {
    !workspaces.is_empty()
        && workspaces.len() <= MAXIMUM_SCOPED_WORKSPACES
        && workspaces.values().map(BTreeSet::len).sum::<usize>() <= MAXIMUM_SCOPED_REPOSITORIES
}

fn invalid_workspace((workspace, repositories): (&String, &BTreeSet<String>)) -> bool {
    invalid_slug(workspace)
        || repositories.is_empty()
        || repositories
            .iter()
            .any(|repository| invalid_slug(repository))
}

fn invalid_slug(value: &str) -> bool {
    value.trim().is_empty()
        || value.len() > MAXIMUM_TEXT_LENGTH
        || value.chars().any(char::is_whitespace)
}

fn validate_capabilities(
    capabilities: &BTreeSet<Capability>,
    expected: IntegrationModuleId,
) -> Result<(), ProfileError> {
    let mismatch = capabilities
        .iter()
        .map(|capability| capability.module())
        .find(|actual| *actual != expected);
    match mismatch {
        Some(actual) => Err(ProfileError::CapabilityModuleMismatch { expected, actual }),
        None => validate_capability_dependencies(capabilities),
    }
}

fn validate_capability_dependencies(
    capabilities: &BTreeSet<Capability>,
) -> Result<(), ProfileError> {
    for capability in capabilities {
        let Some(required) = capability.required_read() else {
            continue;
        };
        if !capabilities.contains(&required) {
            return Err(ProfileError::MissingRequiredCapability {
                capability: *capability,
                required,
            });
        }
    }
    Ok(())
}

fn validate_reports(reports: &ReportsModuleProfile) -> Result<(), ProfileError> {
    if reports.maximum_task_slices <= 1
        || reports.maximum_trend_labels <= 1
        || reports.maximum_team_chart_members <= 1
        || reports.table_page_size == 0
    {
        return Err(ProfileError::InvalidReports);
    }
    Ok(())
}
