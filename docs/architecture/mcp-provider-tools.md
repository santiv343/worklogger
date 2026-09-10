# Provider MCP tools

## Scope

Worklogger exposes everyday operations through build-time add-ons for Jira
Cloud and Bitbucket Cloud. An add-on understands its provider's protocol, not
an organization's structure. Sites, scopes, projects, repositories, branches,
issue types, statuses, fields, and reviewers are discovered or configured data.

Jira is one module. Issues and Time Tracking are internal capability groups
that share a connection, identity, and board. Bitbucket is a separate module
and does not depend on Jira.

There is no universal ticket or pull-request API. Interfaces can combine
information, but each mutation retains the semantics and permissions of the
provider that performs it.

## Design background

The following records the original adapter replacement scope, rather than a
separate compatibility layer:

| Provider | Retained operations | Replaced implementation | Removed assumptions |
| --- | --- | --- | --- |
| Jira | Details, JQL, transitions, comments | Typed responses, discovered editable fields, confirmation, separate capabilities | Fixed custom-field IDs and status names |
| Bitbucket | Details, lists, activity, reviews, creation, comments, approval, closure | Full PR editing, configured scopes, current token authentication | Organization-specific branches, repositories, reviewers, notifications |

## Tool catalog

### Jira

| Tool | Capability | Effect |
| --- | --- | --- |
| `jira_get_issue` | `jira.issue.read` | Read visible issue details |
| `jira_search_issues` | `jira.issue.read` | Run bounded JQL searches with a `hasMore` indicator |
| `jira_get_edit_metadata` | `jira.issue.read` | Read fields Jira permits the account to edit |
| `jira_get_transitions` | `jira.issue.read` | Read currently available transitions |
| `jira_update_issue` | `jira.issue.edit` | Change the confirmed fields |
| `jira_add_comment` | `jira.issue.comment` | Add a comment |
| `jira_transition_issue` | `jira.issue.transition` | Apply a transition by ID |
| `jira_get_my_hours` | `jira.hours.read.self` | Read the authenticated account's hours |
| `jira_get_my_unlogged_issues` | `jira.hours.read.self` | Find assigned sprint issues without the account's worklogs in the period |
| `jira_create_worklog` | `jira.hours.write.self` | Create a worklog for the authenticated account |

Editing accepts a field map because Jira defines standard and custom fields
per project. The agent must query editable metadata first; Worklogger does not
invent IDs or translate status names. Transitions use IDs to avoid ambiguous
text matches.

Creating a worklog requires an RFC 3339 timestamp with the configured timezone
offset and a whole-minute duration. The input never includes an author. The
preview includes the account's matching worklogs for the same issue, date, and
duration; the agent must show these before asking for confirmation. Execution
revalidates the board and the matching worklogs.

### Bitbucket Cloud

| Tool | Capability | Effect |
| --- | --- | --- |
| `bitbucket_list_repositories` | `bitbucket.pr.read` | Read visible repositories in an allowed workspace |
| `bitbucket_list_pull_requests` | `bitbucket.pr.read` | Read filtered PRs in an allowed repository |
| `bitbucket_get_pull_request` | `bitbucket.pr.read` | Read details, participants, and links |
| `bitbucket_get_pull_request_activity` | `bitbucket.pr.read` | Read recent activity |
| `bitbucket_create_pull_request` | `bitbucket.pr.create` | Create a PR with explicit source and destination |
| `bitbucket_update_pull_request` | `bitbucket.pr.edit` | Edit title, description, destination, or source-branch closure setting |
| `bitbucket_add_pull_request_comment` | `bitbucket.pr.comment` | Add a comment |
| `bitbucket_approve_pull_request` | `bitbucket.pr.review` | Approve as the authenticated account |
| `bitbucket_unapprove_pull_request` | `bitbucket.pr.review` | Remove the account's approval |
| `bitbucket_request_changes` | `bitbucket.pr.review` | Request changes as the authenticated account |
| `bitbucket_remove_change_request` | `bitbucket.pr.review` | Remove the account's change request |
| `bitbucket_merge_pull_request` | `bitbucket.pr.merge` | Merge using an explicit strategy |
| `bitbucket_decline_pull_request` | `bitbucket.pr.decline` | Close without merging |

Merge and decline are independent capabilities. Enabling editing or review
does not enable either of them implicitly.

When creating a pull request, Worklogger queries Bitbucket's effective default
reviewers, including repository defaults and inherited project defaults. It
combines them with the requested reviewers without duplicates and shows the
resolved list in the preview. The preview also includes `closeSourceBranch`;
when omitted, it stays `false` and the source branch is kept.

The MCP TUI distributes workflow instructions as three portable `SKILL.md`
directories named `worklogger-*`. It installs them in global Agent Skills,
Codex, Claude Code, and Windsurf destinations. Ownership markers prevent it
from overwriting an unrelated skill with the same name. Cursor uses a different
Rules and Commands model and does not receive an incompatible skill directory.

## Authorization and confirmation

A tool's effective availability follows the intersection defined in the modular
architecture. In addition:

1. Inputs never accept an identity to act as.
2. The adapter obtains the authenticated identity from the provider.
3. Every write starts with a preview of the actor, target, and effect. Alongside
   structured `preview` data, the response includes `visiblePreview` for the
   client to show before requesting confirmation.
4. Execution requires `confirmed: true` and that preview's single-use token.
5. Every write capability also requires read access to the same resource, so
   previewing cannot bypass read permissions.
6. The token is bound to the request and remote state included in the preview.
   Confirmation queries that context again, and the token cannot be reused.
7. The issue must belong to the configured Jira board and the PR to an allowed
   repository.
8. Jira and Bitbucket remain the final authority; a `403` fails closed.
9. Provider tokens never enter MCP JSON, arguments, logs, or responses.

For Bitbucket, the confirmed revision includes the title, description, branches,
source and destination hashes, participants, state, and `updated_on`. Every PR
mutation compares that snapshot again immediately before submission. Jira edits
reread the affected fields and require them to remain unchanged. The providers
do not document a uniform version precondition for these operations; their
conflict response remains the final atomic authority.

MCP annotations supplement this contract: reads are marked `readOnly`, and
writes are marked `destructive` so clients can distinguish them from read-only
observations. The client is responsible for presenting the preview and
obtaining the person's approval.

## Configuration

Secrets are stored separately by provider and purpose. JSON contains only:

- The shared Jira connection: Cloud origin, email, board, and limits, with
  optional Time Tracking configuration nested within the module.
- The Bitbucket Cloud connection: email, allowed workspaces, and limits.
- Enabled modules and capabilities.

Coordinated configuration and credential writes use an exclusive per-user lock
on Windows and compensating rollback, preventing the TUI and Desktop from
interleaving partial updates.

An adapter's official endpoint is a protocol constant, not an organization
customization. It cannot be replaced with an arbitrary URL that could receive
credentials. Jira Data Center, Bitbucket Data Center, and other providers require
separate adapters.

## Delivery sequence

The original implementation sequence was:

1. Define and test the configuration contract and catalog.
2. Implement Jira and test it against isolated HTTP endpoints.
3. Implement Bitbucket Cloud and test it against isolated HTTP endpoints.
4. Add provider and capability selection to the TUI, with management of
   capabilities and configured clients in Desktop.
5. Add isolated MCP tests, builds with excluded add-ons, and optional read-only
   checks against real accounts.
