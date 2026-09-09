# Shared settings

`worklogger-settings` stores the preferences shared by Desktop and MCP in
`settings.json`. On Windows the default directory is `%APPDATA%/Worklogger`;
elsewhere it is `$XDG_CONFIG_HOME/worklogger` or `~/.config/worklogger`.
`WORKLOGGER_SETTINGS_CONFIG` overrides the complete file path for isolated installs.

The versioned document contains Jira connection and board fields, Jira limits,
hours preferences, report preferences, Bitbucket scope and pull-request defaults.
Fields may be absent while the user edits an incomplete configuration. Presence
does not imply that a connection was verified. Provider adapters must validate
credentials, scope, and organization policy before using the settings.

MCP grants live in a separate optional `mcp` namespace. Desktop settings do not
create grants. Credentials remain in the existing secure stores or
environment; the shared schema rejects unknown fields, including token fields.

Every save checks the revision observed by the caller while holding a writer
lock, writes atomically, and returns the incremented revision. A stale editor
receives `SettingsError::Conflict` and must reload before saving. A competing
writer receives `SettingsError::Busy`. The lock is owned by the operating
system, so it is released automatically if a process exits unexpectedly. The
document remains either the previous or newly committed version.

Worklogger reads and writes this one document only. It does not import, merge,
or modify previous per-frontend configuration files.
