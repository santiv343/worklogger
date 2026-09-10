# Worklogger

> **Installing with an AI assistant?** Start with the [assistant guide](docs/assistant-guide.md).

Track Jira time in a desktop app, or work with Jira and Bitbucket from your AI
assistant. The base distribution contains no organization names, logos, boards,
URLs, or credentials.

## Quick start

Worklogger MCP supports Windows x64 and Linux x64 (including WSL, glibc 2.35+).
Install it from the same environment where the MCP client runs. Node.js 18 or
newer is required only to launch the public npm installer.

For the guided path, run this and follow the terminal steps:

```shell
npx @santiv343/worklogger
```

For a reviewed, read-only Jira configuration, copy
[`config/example.jira-readonly.mcp.json`](config/example.jira-readonly.mcp.json)
to a private location, replace its example values, then run the installer from
the same shell where the token is entered:

```shell
read -rsp "Jira API token: " WORKLOGGER_JIRA_API_TOKEN; echo
export WORKLOGGER_JIRA_API_TOKEN
npx --yes @santiv343/worklogger install \
  --config /private/path/mcp.json \
  --clients codex \
  --yes
unset WORKLOGGER_JIRA_API_TOKEN
```

Restart the registered client, run `npx @santiv343/worklogger status`, and make
one read-only request such as “Show my worklogs for this week.” For Desktop,
MCP, and troubleshooting instructions, see the [user guide](docs/user-guide/README.md).

Desktop and MCP share `settings.json` for provider preferences. API tokens and
MCP grants are deliberately separate: configuring Desktop never enables tools
for an assistant. The interface language is also shared: a change in the MCP
TUI applies immediately and persists for future launches. Restart Desktop after
changing the language there.
Worklogger uses this one settings document and does not import or modify older
per-frontend configuration files.

The current version can:

- read, create, edit, and delete a person's own Jira worklogs;
- browse periods, daily workload, tasks, and weekly progress;
- discover boards available to the authenticated account;
- read personal reports and, when authorized, team reports;
- export traceable XLSX and PDF reports;
- expose selected capabilities through a standalone MCP server;
- install or remove that server from detected MCP clients; and
- apply branding, limits, and policies through an organization profile.

## Editions

The same codebase produces two distribution types:

| Edition | Organization configuration | Use |
| --- | --- | --- |
| Community | Manual or JSON import/export | Generic product and development |
| Managed | Immutable JSON profile embedded at build time | Organization-ready distribution |

A Managed edition does not include the configuration import/export interface and
does not read external files at runtime. Personal preferences and credentials
remain private to each user.

Profiles never contain API tokens. Jira determines the effective identity and
permissions; Worklogger never manages another person's worklogs.

## Shared modular configuration

`organization.json` is Worklogger's only shareable file. Community Desktop can
export it with **Export JSON** and teammates can import it with **Import JSON**
or use it in `npx @santiv343/worklogger setup --profile <file>`. It contains
branding and an optional section for every module under `modules`; when a
section is absent, that organization does not offer the module. The file can
include scopes, limits, and capabilities, but never identity, email addresses,
tokens, or session state.

A feature is available only when its add-on is compiled, its section is present
in the profile, the user enables it, and the authenticated account has access
within the configured scope. Local configuration can never expand provider
permissions.

Community Desktop and the MCP TUI use the same installed profile and a shared,
secret-free `settings.json` document in the user's
configuration directory (`%APPDATA%\Worklogger` on Windows or
`$XDG_CONFIG_HOME/worklogger`/`~/.config/worklogger` on Linux). It is
secret-free, but contains personal choices such as an account email, selected
board, and MCP consent. Do not use it as a team template: share only a reviewed
`organization.json`. A Managed edition embeds the same schema at build time and
keeps it immutable for both Desktop and the MCP sidecar.

Each provider scope is explicit: `restricted` requires allowed sites or
workspaces, while `unrestricted` deliberately requires an empty list. Profile
capabilities and `maximumAllowed*` fields are organization limits; the remaining
limits are defaults that a user may lower. The current Desktop experience
requires Jira because time tracking is its primary experience, while Reports may
be omitted. Standalone MCP supports Jira-only, Bitbucket-only, and no-add-on
combinations.

## Development

Requires Rust 1.88 and Dioxus CLI 0.7.9:

```bash
rustup toolchain install 1.88.0 --profile minimal --component clippy,rustfmt
cargo install dioxus-cli --version 0.7.9 --locked
./dev-desktop
```

To use a local profile without adding it to the repository:

```bash
cp config/example.organization.json config/worklogger.local.json
./dev-desktop
```

`dev-desktop` also builds the MCP server and makes its path available to the
Configuration → MCP screen. Source changes continue to use hot reload.

## Optional MCP

`worklogger-mcp` is independent from Desktop and always operates as the
authenticated provider account. Jira is one module: Issues and Time Tracking are
internal capability groups that share a connection and scope but keep separate
services. Bitbucket is another module and can be built without Jira. Time
tracking never accepts a person as a parameter and never changes another
person's worklogs.

The current catalog includes:

- Jira Cloud: issue details and JQL search, editable metadata, transitions,
  field edits, and comments;
- Jira time tracking: confirmed read/write of own worklogs, plus detection of
  assigned sprint issues without the person's worklogs during a period; and
- Bitbucket Cloud: repositories, pull-request list/detail/activity, creation,
  edits, comments, approval, change requests, merge, and decline.

Every group has its own capability. Merge and decline are independent from edit
and review; disabling a capability removes its tools from the MCP handshake.
Every write first returns a preview with actor, target, effect, and one-time
token. It runs only when the same request is sent again with `confirmed: true`
and that token. The state from the preview is queried again; a changed state
invalidates confirmation. Pull-request mutations compare state, branches,
hashes, and revision again immediately before writing. No sites, workspaces,
repositories, branches, statuses, reviewers, or fields are predefined for a
specific company.

`jira_create_worklog` accepts an issue, RFC 3339 timestamp with the configured
offset, whole-minute duration, and optional comment. It does not accept an
author. The preview reports matching worklogs for the same issue, date, and
duration to prevent accidental duplicates. `jira_search_issues` returns at most
the requested limit and indicates additional pages with `hasMore`.

Missing-worklog detection is Jira-only. When Bitbucket is installed and enabled,
its pull requests and activity can provide extra evidence, but are never needed
to calculate Jira candidates.

Desktop Configuration → MCP can enable capabilities and register or remove
Worklogger from detected clients. Before changing a client, it shows the exact
target and scope and asks for confirmation.

The package is public and does not require a Worklogger account or token. Open
the TUI from PowerShell, Linux, or WSL:

```shell
npx @santiv343/worklogger
```

If this scope was previously configured for GitHub Packages, run
`npm config delete @santiv343:registry` once to return to the public npm registry.

To install a shared team profile and open the guided setup:

```shell
# Windows
npx @santiv343/worklogger setup --profile C:\path\organization.json

# Linux or WSL
npx @santiv343/worklogger setup --profile /path/organization.json
```

Available commands:

```shell
npx @santiv343/worklogger clients
npx @santiv343/worklogger skills
npx @santiv343/worklogger status
npx @santiv343/worklogger uninstall
```

### Manual or automated installation

In addition to the TUI, a reviewed configuration can be installed without
interaction. Start from [`config/example.mcp.json`](config/example.mcp.json),
keep the person's values outside the repository, and provide tokens through
environment variables or the operating system's secure store:

```shell
read -rsp "Jira API token: " WORKLOGGER_JIRA_API_TOKEN; echo
export WORKLOGGER_JIRA_API_TOKEN
read -rsp "Bitbucket API token: " WORKLOGGER_BITBUCKET_API_TOKEN; echo
export WORKLOGGER_BITBUCKET_API_TOKEN
npx --yes @santiv343/worklogger install \
  --config /private/path/mcp.json \
  --clients codex,claude-code \
  --skills \
  --yes
unset WORKLOGGER_JIRA_API_TOKEN WORKLOGGER_BITBUCKET_API_TOKEN
```

Use `--clients all` for every detected client. The command never replaces an
unrelated or invalid registration, never accepts tokens as arguments, and makes
no client changes when it finds a conflict. When it finishes, restart the MCP
client and run `npx @santiv343/worklogger status` to verify the result.

The TUI opens direct Settings. Jira and Bitbucket each have independent
connection, scope, permission/default, and advanced-limit sections; MCP clients
and assistant skills are available from the same settings menu. In Jira, Time
Tracking and Issues are groups within the same module. Editing one section
preserves all other shared preferences and MCP consent. Restart an MCP client
after changing its settings.

`organization.json` is the shareable source of modules, maximum scopes, limits,
and allowed capabilities. `settings.json` contains local shared preferences and
MCP consent but never tokens; it should stay private because it may include
personal choices. Worklogger does not import or modify earlier `mcp.json` and
`config.json` files. Tokens remain outside JSON:
Windows uses Credential Manager and Linux uses an atomic per-user store protected
by `0700/0600` permissions. Desktop and MCP credentials stay separate.

The `skills` command installs or updates Worklogger's workflow skills for
supported assistants. It manages only `worklogger-jira`, `worklogger-daily`, and
`worklogger-delivery`; if it finds one of those directories not created by
Worklogger, it does not replace it.

For development, the full catalog can start from
[`config/example.mcp.json`](config/example.mcp.json), point to its location, and
receive secrets only through environment variables:

```powershell
$env:WORKLOGGER_MCP_CONFIG = "C:\path\mcp.json"
$env:WORKLOGGER_JIRA_API_TOKEN = "<jira-token>"
$env:WORKLOGGER_BITBUCKET_API_TOKEN = "<bitbucket-token>"
worklogger-mcp serve
```

Bitbucket uses scoped API tokens, the Atlassian account email address, and the
official Bitbucket Cloud endpoint. Its endpoint cannot be replaced by an URL
that could receive credentials. Jira Data Center and Bitbucket Data Center need
different add-ons.

Worklogger currently detects Codex, Claude Code, Claude Desktop, Cursor,
Windsurf, Qwen Code, Gemini CLI, Kiro, GitHub Copilot, and Trae Code CLI. Most
use the common JSON `mcpServers` format; Codex and Trae Code CLI use native CLI
and TOML adapters respectively. See the [MCP client support guide](docs/mcp-client-support.md)
for the exact targets, project-scoped Trae IDE setup, and skill compatibility.
The executable is installed in a versioned location:
`%LOCALAPPDATA%\Worklogger\MCP` on Windows and
`$XDG_DATA_HOME/worklogger/MCP` or `~/.local/share/worklogger/MCP` on Linux/WSL.
Desktop and the TUI register the corresponding native path. Each version is
isolated so clients already in use are not broken. Node is not a runtime
dependency after installation.

Run the installer in the same environment as the client: inside WSL for Codex
or Claude Code installed there, and from PowerShell for Windows applications.
No WSL/Windows interoperability is required. The Linux x64 distribution is
built and tested on Ubuntu 22.04, with glibc 2.35 as the minimum supported base.

After a server update, every MCP client must restart to end the previous process
and start a handshake with the new path.

On update, Worklogger distinguishes its own prior registration from an unrelated
conflict: the former can be repaired or removed, while the latter is never
removed without explicit replacement confirmation. It also recognizes the usual
Windows npm installation of Codex (`codex.cmd`) but runs its entry point directly
with Node instead of passing arguments through a shell. No client file receives
Jira or Bitbucket tokens.

## Verification

```bash
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --package worklogger-desktop --no-default-features --locked
cargo check --package worklogger-desktop --no-default-features --features hours --locked
```

## Creating Windows distributions

From PowerShell:

```powershell
# Generic edition
.\scripts\build-windows.ps1 -Edition Community

# Jira-only Desktop and MCP
.\scripts\build-windows.ps1 -Edition Community -McpAddons jira

# Immutable organization edition
.\scripts\build-windows.ps1 `
  -Edition Managed `
  -Profile C:\profiles\company.json `
  -Name Company
```

To build both in one run:

```powershell
.\scripts\build-release-set.ps1 `
  -ManagedProfile C:\profiles\company.json `
  -ManagedName Company
```

Each run writes the installer, portable ZIP, and SHA-256 to `dist/`. See
[Custom distributions](docs/architecture/custom-distributions.md) for the full guide.

## Documentation

- [Desktop and MCP user guide](docs/user-guide/README.md)
- [Assistant installation guide](docs/assistant-guide.md)
- [Changelog](CHANGELOG.md)
- [Modular architecture](docs/architecture/modularity.md)
- [Jira and Bitbucket MCP tools](docs/architecture/mcp-provider-tools.md)
- [Custom distributions](docs/architecture/custom-distributions.md)
- [Permissions and trust boundaries](docs/security/permission-model.md)
- [ADR: one codebase, multiple editions](docs/adr/0001-single-codebase-distributions.md)
- [ADR: vertical-slice neutrality](docs/adr/0002-provider-neutral-vertical-slices.md)
- [ADR: provider-native MCP mutations](docs/adr/0003-provider-native-mcp-mutations.md)
- [ADR: modular organization profile](docs/adr/0004-modular-organization-profile.md)
- [Versioning and releases](docs/architecture/versioning-and-releases.md)

## Technology

- Rust 2024
- Dioxus Desktop 0.7
- WebView2 on Windows
- Jira Cloud REST API
- Bitbucket Cloud REST API
- Windows Credential Manager or a private Linux secret store

The bootstrap package is publicly distributed through npm. Tags publish Windows
and Linux MCP servers together with that package; Windows Desktop installers
remain workflow artifacts until an explicit release is created.
