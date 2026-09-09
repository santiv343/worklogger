use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{AccountId, Duration, IssueKey};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Worklog {
    pub id: String,
    pub issue_key: IssueKey,
    pub issue_summary: String,
    pub author: AccountId,
    pub started: OffsetDateTime,
    pub duration: Duration,
    pub comment: String,
    pub issue_url: String,
}

impl Worklog {
    #[must_use]
    pub fn belongs_to(&self, account_id: &AccountId) -> bool {
        self.author == *account_id
    }
}
