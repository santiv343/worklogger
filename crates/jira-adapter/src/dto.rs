use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JiraUserDto {
    pub account_id: String,
    pub display_name: String,
    pub active: bool,
    pub account_type: Option<String>,
    pub email_address: Option<String>,
    pub time_zone: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardDto {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub board_type: String,
    #[serde(rename = "self")]
    pub self_url: String,
    pub location: Option<BoardLocationDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardLocationDto {
    pub project_id: Option<u64>,
    pub project_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MyPermissionsDto {
    pub permissions: HashMap<String, PermissionDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionDto {
    pub have_permission: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct IssueDto {
    pub id: String,
    pub key: String,
    #[serde(rename = "self")]
    pub self_url: String,
    pub fields: IssueFieldsDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct IssueFieldsDto {
    pub summary: String,
    #[serde(default)]
    pub assignee: Option<JiraUserDto>,
    #[serde(default, rename = "issuetype")]
    pub issue_type: Option<NamedFieldDto>,
    #[serde(default)]
    pub status: Option<NamedFieldDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct NamedFieldDto {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct JiraIssueDocumentDto {
    pub id: String,
    pub key: String,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct JiraEditMetadataDto {
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JiraTransitionDto {
    pub id: String,
    pub name: String,
    pub to: JiraTransitionTargetDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JiraTransitionTargetDto {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JiraCommentDto {
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JiraIssueSearchPageDto {
    pub is_last: bool,
    pub next_page_token: Option<String>,
    pub issues: Vec<JiraIssueDocumentDto>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JiraTransitionsDto {
    pub transitions: Vec<JiraTransitionDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JiraIssueSearchRequestDto<'request> {
    pub jql: &'request str,
    pub fields: &'request [String],
    pub max_results: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<&'request str>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JiraIssueUpdateRequestDto<'request> {
    pub fields: &'request BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JiraCommentRequestDto {
    pub body: AdfDocumentDto,
}

#[derive(Debug, Serialize)]
pub(crate) struct JiraTransitionRequestDto<'request> {
    pub transition: JiraTransitionIdDto<'request>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JiraTransitionIdDto<'request> {
    pub id: &'request str,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorklogDto {
    pub id: String,
    pub issue_id: Option<String>,
    pub author: WorklogAuthorDto,
    pub started: String,
    pub time_spent_seconds: u32,
    pub comment: Option<Value>,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub visibility: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorklogAuthorDto {
    pub account_id: String,
    pub display_name: String,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BoardPageDto {
    pub start_at: u32,
    pub total: u32,
    pub values: Vec<BoardDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IssuePageDto {
    pub is_last: bool,
    pub next_page_token: Option<String>,
    pub issues: Vec<IssueDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorklogPageDto {
    pub start_at: u32,
    pub total: u32,
    pub worklogs: Vec<WorklogDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorklogRequestDto {
    pub started: String,
    pub time_spent_seconds: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<AdfDocumentDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AdfDocumentDto {
    #[serde(rename = "type")]
    kind: &'static str,
    version: u8,
    content: [AdfParagraphDto; 1],
}

#[derive(Debug, Serialize)]
pub(crate) struct AdfParagraphDto {
    #[serde(rename = "type")]
    kind: &'static str,
    content: [AdfTextDto; 1],
}

#[derive(Debug, Serialize)]
pub(crate) struct AdfTextDto {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

impl AdfDocumentDto {
    pub(crate) fn plain_text(text: String) -> Self {
        let content = [AdfTextDto { kind: "text", text }];
        Self {
            kind: "doc",
            version: 1,
            content: [AdfParagraphDto {
                kind: "paragraph",
                content,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JiraUserDto, WorklogRequestDto};

    #[test]
    fn accepts_identity_with_private_email() {
        let body = r#"{"accountId":"abc","displayName":"Ana","active":true}"#;
        let user: JiraUserDto = serde_json::from_str(body).expect("valid identity");
        assert_eq!(user.email_address, None);
    }

    #[test]
    fn worklog_request_never_contains_an_author() {
        let request = WorklogRequestDto {
            started: "2026-09-01T12:00:00.000-0300".to_owned(),
            time_spent_seconds: 3_600,
            comment: Some(super::AdfDocumentDto::plain_text("Trabajo".to_owned())),
        };
        let value = serde_json::to_value(request).expect("serializable request");
        assert!(value.get("author").is_none());
        assert_eq!(value["comment"]["type"], "doc");
    }
}
