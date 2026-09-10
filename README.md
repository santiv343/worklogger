# Worklogger

> Personal Jira time tracking and reviewed Jira and Bitbucket workflows for AI assistants.

[Install MCP](#install-mcp) · [Use Desktop](docs/user-guide/README.md#use-desktop) · [Documentation](docs/README.md) · [Troubleshooting](docs/user-guide/README.md#security-and-troubleshooting)

Worklogger brings three kinds of everyday work into one local tool:

- Jira worklogs and reports in a Desktop app;
- Jira issues and worklogs in an AI assistant; and
- Bitbucket pull-request work in an AI assistant.

It runs on your computer and connects directly to the provider accounts you
choose. There is no Worklogger account, shared cloud workspace, or built-in
organization configuration.

## What you can do

**Keep your time up to date.** Review this week's worklogs, find assigned
issues without time, and create or edit only your own Jira worklogs.

**Prepare Jira changes with an assistant.** Read an issue, inspect its editable
fields or transitions, and prepare a comment or update for review.

**Prepare pull requests deliberately.** Inspect allowed repositories, branches,
reviewers, and pull-request activity before creating, editing, reviewing, or
merging a pull request.

## Choose a surface

| Surface | Best for | Start here |
| --- | --- | --- |
| Desktop | Logging personal time, browsing work, and reading reports | [Desktop guide](docs/user-guide/README.md#use-desktop) |
| MCP | Using Jira and Bitbucket from Codex, Claude, Cursor, and other supported clients | [Install MCP](#install-mcp) |

Desktop and MCP share local preferences such as language and provider choices.
They do not share credentials or automatically grant one surface access because
the other was configured.

## Install MCP

Worklogger MCP supports Windows x64 and Linux x64, including WSL on a supported
Linux distribution. Node.js 18 or newer is needed only to start the installer.
Run it in the same environment as the client that will use MCP: PowerShell for a
Windows app, or the corresponding WSL terminal for a WSL client.

```shell
npx @santiv343/worklogger
```

The guided setup will:

1. connect and validate Jira and/or Bitbucket with credentials entered locally;
2. let you choose the board, repositories, capabilities, and limits to enable;
3. register the local server in a supported MCP client; and
4. show the configuration state and a way to verify the connection.

Restart the client after installation, then try a read-only request such as:

> Show my worklogs for this week.

For scripted installs, client support, and update instructions, use the
[MCP guide](docs/user-guide/README.md#use-mcp).

## How provider changes are controlled

Worklogger does not submit a write as soon as an assistant requests one. The
server first returns the proposed actor, target, effect, and a one-time preview
token. A write can run only when the unchanged request is sent again with that
token and explicit confirmation. If the relevant issue or pull request changed,
the preview is rejected and must be created again.

The assistant client receives provider data returned by MCP. Its handling depends
on the client and model service you choose. Read the [permission and privacy
model](docs/security/permission-model.md) before enabling write capabilities.

## Credentials and team configuration

Provider credentials are stored locally: Windows uses Credential Manager and
Linux uses a private, permission-restricted user store. Tokens are never written
to Worklogger settings JSON, MCP client configuration, or a command argument.

Teams can share a reviewed `organization.json` profile with allowed providers,
scopes, limits, and capabilities. Each person still connects with their own
account and credentials. Do not share `settings.json`, `mcp.json`, client
configuration, or tokens. See the [user guide](docs/user-guide/README.md) for
the setup flow.

## Documentation

- [User guide](docs/user-guide/README.md): Desktop, MCP, settings, teams, and troubleshooting.
- [Guide for assistants](docs/assistant-guide.md): install and verify MCP without asking for a token in chat.
- [Supported MCP clients](docs/mcp-client-support.md): client-specific registration targets.
- [Permissions and privacy](docs/security/permission-model.md): provider scope, confirmation, and credential boundaries.
- [Architecture and release notes](docs/README.md): implementation and contributor references.

## Contributing

The product is written in Rust. Development requires Rust 1.88 and Dioxus CLI
0.7.9. See the [documentation hub](docs/README.md) before building from source.

## License

[MIT](LICENSE)
