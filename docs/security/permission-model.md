# Permissions and privacy

Worklogger is a local connection between the authenticated provider account and
the Desktop app or an MCP client. This page describes the boundaries that remain
in force after installation.

## What Worklogger can access

An operation is available only when all of these controls allow it:

1. the installed binary includes the relevant provider module;
2. an organization profile, if present, allows that module and scope;
3. the person enabled the capability locally;
4. Jira or Bitbucket grants the authenticated account permission; and
5. the target resource is inside the configured scope and passes ownership
   checks where applicable.

An organization profile can narrow configuration. It cannot grant a provider
permission that the authenticated account does not have.

The local setup uses personal API tokens where organizational policy permits
them. It avoids a separate shared Worklogger integration; it does not bypass
provider access controls or token scopes.

## Supported API tokens

**Jira Cloud:** the current connector uses an API token **without scopes**
against your `https://<site>.atlassian.net` address. Scoped Jira API tokens
require a different API route that Worklogger does not support yet. If your
organization requires scoped tokens or prohibits unscoped tokens, the current
Jira connector does not meet that policy. See Atlassian's
[account API-token guide](https://support.atlassian.com/atlassian-account/docs/manage-api-tokens-for-your-atlassian-account/).

**Bitbucket Cloud:** use a separate API token with scopes for the enabled
operations. Jira's token limitation does not apply to the Bitbucket connector.
See the [Bitbucket API-token guide](https://support.atlassian.com/bitbucket-cloud/docs/api-tokens/).

## Worklogs and identity

Worklogger gets identity from the authenticated provider account. Creating a
worklog never accepts an author as an input. Updating or deleting a worklog
checks ownership again before the provider request. Team time data, when a
provider and edition allow it, is read-only.

## MCP previews and confirmation

For a provider write, the MCP server returns the actor, target, intended effect,
and a one-time confirmation token. The server accepts the write only when the
same request is submitted again with that token and explicit confirmation. It
rechecks relevant provider state before the write; a changed resource invalidates
the preview.

This is a server control. The assistant client is still responsible for showing
the proposed change and asking the person to approve it before submitting the
confirmation request.

## Credentials and local data

Windows stores provider tokens in Credential Manager. Linux stores them in a
private per-user directory with `0700` directory permissions and `0600` token
files. Tokens are not stored in settings JSON, organization profiles, client
configuration, logs, reports, or error messages.

`organization.json` is the only intended team-shareable configuration. It can
describe provider modules, scopes, limits, and allowed capabilities, but never
accounts or tokens. `settings.json` is token-free but may include personal
preferences, selected boards, and MCP consent, so it should remain private.

## Data sent to assistants

Jira and Bitbucket data returned by MCP is available to the assistant client.
How that client, its extensions, and its selected model service handle the data
is outside Worklogger's control. Review the data policy of the client and model
service before enabling MCP capabilities.

## Client registration

The installer registers only its own `worklogger` MCP entry after showing the
target and obtaining confirmation. It preserves unrelated client configuration
and refuses to overwrite a conflicting entry. Client configuration contains the
local executable path and `serve` argument, never provider credentials.

Removing Worklogger removes only the registration it owns, its MCP settings, and
its MCP credential. It does not change Desktop credentials, provider data, or
unrelated integrations.
