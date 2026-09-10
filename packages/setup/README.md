# Worklogger MCP

Use Jira worklogs, issue updates, and Bitbucket pull requests from an AI
assistant. Worklogger runs a local MCP server and lets you decide which provider
tools the assistant can call.

It is useful for three everyday jobs:

- check this week's worklogs and find assigned issues without time;
- read an issue, prepare a comment or field update, then review the change; and
- prepare a pull request with its reviewers and branch settings before creating it.

Every provider write returns a preview and requires a matching confirmation
request. Worklogger stores credentials locally: Windows uses Credential Manager
and Linux uses a private, permission-restricted user store. Tokens are never
written to generated JSON or client configuration.

The public installer supports Windows x64 and Linux x64 with glibc 2.35 or
newer, including Ubuntu 22.04+ and WSL2. It contains the native server for the
current system and installs it in a versioned user location:

- Windows: `%LOCALAPPDATA%\Worklogger\MCP\<version>`;
- Linux/WSL: `$XDG_DATA_HOME/worklogger/MCP/<version>` or
  `~/.local/share/worklogger/MCP/<version>`.

Node.js 18 or newer is required only to launch the public installer; Node is not
needed while the server is running.

## Guided setup

Run the installer in the same environment as the client: PowerShell for Windows
applications, or the relevant WSL terminal for Codex or Claude Code in WSL.

```shell
npx @santiv343/worklogger
```

If this scope was previously configured for GitHub Packages, return to the
public npm registry once:

```shell
npm config delete @santiv343:registry
```

Without arguments, Worklogger opens a guided terminal interface. It shows what
is configured and missing, validates each provider as you connect it, and lets
you install the server, clients, and optional assistant skills separately.

After setup, restart the registered client and try a read-only request such as:

> Show my worklogs for this week.

To use a shared team profile:

```shell
npx @santiv343/worklogger setup --profile /path/organization.json
```

The profile is validated and installed in the user's configuration directory. It
may define branding, modules, scopes, limits, and allowed capabilities, but never
accounts or secrets. The wizard stores every person's token in Windows Credential
Manager or a Linux atomic store protected by `0700/0600`; no token is copied to
JSON or MCP clients.

## Manual or headless setup

Copy a reviewed `mcp.json` to a private location. Start from the read-only Jira
example in the repository, enter any required token in the same shell without
putting it in shell history, then run:

```shell
read -rsp "Jira API token: " WORKLOGGER_JIRA_API_TOKEN; echo
export WORKLOGGER_JIRA_API_TOKEN
npx --yes @santiv343/worklogger install \
  --config /private/path/mcp.json \
  --clients codex \
  --yes
unset WORKLOGGER_JIRA_API_TOKEN
```

The first `--yes` accepts npm's install prompt. The final `--yes` confirms
Worklogger changes. `--clients` accepts `all`, `codex`, `claude-code`,
`claude-desktop`, `cursor`, `windsurf`, `qwen-code`, `gemini-cli`, `kiro`,
`github-copilot`, and `trae-code`. See the [MCP client support guide](../../docs/mcp-client-support.md)
for the target configuration used by each client. Add `--skills` only when
workflow skills should be installed for every compatible assistant destination
detected on that account.

The command never replaces unrelated or invalid client registrations. Restart
registered clients, run `npx @santiv343/worklogger status`, and make one
read-only request through the client to verify the connection.

## Other commands

```shell
npx @santiv343/worklogger clients
npx @santiv343/worklogger skills
npx @santiv343/worklogger status
npx @santiv343/worklogger uninstall
```

`uninstall` removes only registrations owned by Worklogger, its MCP
configuration, and its MCP credential. It does not touch Desktop credentials,
unrelated integrations, Jira worklogs, pull requests, or versioned binaries that
may still be running.

## Learn more

- [Install and use Worklogger](https://github.com/santiv343/worklogger/blob/main/docs/user-guide/README.md)
- [Supported MCP clients](https://github.com/santiv343/worklogger/blob/main/docs/mcp-client-support.md)
- [Guide for assistants](https://github.com/santiv343/worklogger/blob/main/docs/assistant-guide.md)
- [Permissions and privacy](https://github.com/santiv343/worklogger/blob/main/docs/security/permission-model.md)
