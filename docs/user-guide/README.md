# Worklogger user guide

Worklogger has two complementary uses:

- **Desktop:** view tasks, log time, and read reports.
- **MCP:** make those capabilities available to an assistant such as Codex or Claude.

MCP uses your own Jira or Bitbucket account and permissions. Never share API tokens: Worklogger keeps them in your operating system's secure store and never writes them to client configuration files.

![Worklogger Desktop walkthrough](assets/desktop-flow.svg)

## Choose a path

| Goal | Start here |
| --- | --- |
| Log time and read reports | [Use Desktop](#use-desktop) |
| Ask an assistant about work | [Use MCP](#use-mcp) |
| Use both | Configure either surface; shared preferences are available to both |

## Use Desktop

### Connect Jira

1. Open Worklogger and choose **Connect Jira**.
2. Choose **Import JSON** if your team gave you an `organization.json`; otherwise configure the connection manually. Community Desktop creates this safe team file through **Export JSON**.
3. Enter your Jira site, Atlassian email address, and API token.
4. Choose **Verify and find boards**, then select a board, weekly target, and time zone.
5. Choose **Save and continue**.

The selected board defines the scope used to read tasks and create worklogs. Worklogger verifies the account before it lets you select one.

### Log time and read reports

Open **Jira** and choose **+ Log time**. Select an issue, enter a date and duration, optionally add a comment, then review and confirm. Worklogger creates the worklog only for your authenticated account. You can browse weeks and edit or delete your own worklogs.

If your edition includes **Reports**, choose a period from the sidebar. Personal reports contain only your worklogs. Team reports appear only when enabled and authorized, and are always read-only. Both can be exported to XLSX or PDF.

### Enable MCP from Desktop

1. Open **Configuration → MCP**.
2. Enable only the capabilities you need.
3. Under **MCP clients**, choose a client that has been opened at least once on that computer.
4. Choose **Install**, review the target, and confirm.
5. Restart the client.

Installation changes only the selected client's `worklogger` entry. It does not add tokens or alter other integrations.

### Choose a language

Open **Configuration → General** and choose **English** or **Spanish**. The
choice is saved in the shared preferences, so the MCP TUI uses it too. The TUI
updates immediately; restart Desktop after changing its language.

## Use MCP

MCP connects Worklogger to supported clients including Codex, Claude Code,
Claude Desktop, Cursor, Windsurf, Qwen Code, Gemini CLI, Kiro, GitHub Copilot,
and Trae Code CLI. The server runs locally and exposes only the capabilities
you enable.

![MCP setup and confirmation walkthrough](assets/mcp-flow.svg)

### Guided installation

Run the installer in the same environment as the client: PowerShell for Windows applications, or the relevant WSL terminal for Codex or Claude Code inside WSL.

```shell
npx @santiv343/worklogger
```

Open **Settings**. Choose **Language** at any time, then configure Jira or Bitbucket in its own sections, choose only
the MCP permissions you need, then use **MCP clients** and **Assistant skills**
from that same menu. Each section shows whether it still needs configuration,
saves only its own values, and preserves the rest. Restart the client when it
finishes.

For a shared profile:

```shell
npx @santiv343/worklogger setup --profile /path/organization.json
```

### Manual or headless installation

Use this only after a person has reviewed a configuration. Copy [`config/example.mcp.json`](../../config/example.mcp.json) to a private location, fill in real values, and keep tokens out of the file.

```shell
export WORKLOGGER_JIRA_API_TOKEN="<jira-token>"
export WORKLOGGER_BITBUCKET_API_TOKEN="<bitbucket-token>"
npx --yes @santiv343/worklogger install \
  --config /private/path/mcp.json \
  --clients codex,claude-code \
  --skills \
  --yes
```

In PowerShell, define `$env:WORKLOGGER_JIRA_API_TOKEN = "<jira-token>"` before the command. `--clients` accepts `all`, `codex`, `claude-code`, `claude-desktop`, `cursor`, `windsurf`, `qwen-code`, `gemini-cli`, `kiro`, `github-copilot`, and `trae-code`; `--skills` is optional.

The first `--yes` accepts npm's first-install prompt; the final `--yes` confirms Worklogger. The command saves local configuration, installs the local server, and registers the selected clients. It stops rather than replacing an unrelated or invalid MCP registration. `--skills` installs skills for every compatible assistant destination detected on that account, independently of `--clients`. Restart registered clients, then verify:

```shell
npx @santiv343/worklogger status
```

## Commands and skills

| Command | Purpose |
| --- | --- |
| `npx @santiv343/worklogger` | Open interactive settings. |
| `npx @santiv343/worklogger status` | View modules, server, and detected clients. |
| `npx @santiv343/worklogger clients` | Install or remove Worklogger from a client. |
| `npx @santiv343/worklogger skills` | Install or update workflow skills. |
| `npx @santiv343/worklogger uninstall` | Remove only Worklogger's MCP registration and configuration. |

The `skills` command installs `worklogger-jira`, `worklogger-daily`, and `worklogger-delivery` for shared Agent Skills, Codex, Claude Code, and Windsurf without modifying unrelated skills. Cursor uses Rules and Commands instead.

To let an assistant guide installation, share the repository link and ask it to follow the [assistant guide](../assistant-guide.md).

## Ask an assistant to act

After registration and restart, use plain language: “Show my worklogs for this week”, “Find issue PROJ-123 and summarize it”, or “List open pull requests in the allowed repository”.

Every action that changes data must show a visible **Preview** with identity, target, and effect, then ask for explicit confirmation. A preview is single-use: if the issue or pull request changes, Worklogger rejects stale confirmation.

For pull requests, Worklogger merges effective Bitbucket default reviewers with explicit reviewers without duplicates. The preview says whether the source branch will close; if unspecified, it remains open.

## Security and troubleshooting

- Never paste tokens into chat, tickets, `organization.json`, or MCP client configuration files.
- Share a reviewed `organization.json` with a team. `settings.json` is
  secret-free but contains personal choices and consent, so keep it private.
  `mcp.json` and client configuration files are also local-only.
- A profile can restrict scopes but cannot grant provider permissions.
- Restart the MCP client after updating Worklogger.

| Situation | What to do |
| --- | --- |
| My client does not appear | Open it once and rerun detection in the same environment. |
| A tool does not appear | Check that its module is bundled, profile-allowed, and enabled; then restart the client. |
| An action is rejected after confirmation | Query again and create a new preview; the resource or permissions may have changed. |
| I want to remove MCP | Use Desktop **Remove** or `npx @santiv343/worklogger uninstall`. |
