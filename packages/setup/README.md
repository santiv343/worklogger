# Worklogger MCP setup

Public CLI for configuring and managing the standalone MCP server on Windows x64
or Linux x64 with glibc 2.35 or newer, including Ubuntu 22.04+ and WSL2. It
contains both Rust binaries and runs the native binary for the current system.
The server is installed in a versioned user location:

- Windows: `%LOCALAPPDATA%\Worklogger\MCP\<version>`;
- Linux/WSL: `$XDG_DATA_HOME/worklogger/MCP/<version>` or
  `~/.local/share/worklogger/MCP/<version>`.

Node is used only to launch the installer; it is not required while the server
is running. Node.js 18 or newer is required to run `npx`.

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

Without arguments, Worklogger opens its TUI. It shows configuration, local
server availability, active modules, and detected clients, and provides guided
setup, registration, removal, and skill installation.

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

For the complete product guide, see the [repository user guide](https://github.com/santiv343/worklogger/blob/main/docs/user-guide/README.md). Assistants should follow the [assistant guide](https://github.com/santiv343/worklogger/blob/main/docs/assistant-guide.md).
