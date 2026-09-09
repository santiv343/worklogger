# Worklogger guide for assistants

Use this guide when helping a person install or use Worklogger MCP from this public repository.

## Safety rules

- Never ask for, repeat, store, or commit API tokens in chat, source code, logs, pull requests, or shared files.
- A token may be supplied only locally through the interactive installer, an existing operating-system secure store, or a temporary environment variable in the same shell that runs Worklogger.
- Do not add `--yes` until the person has reviewed the configuration and selected clients. It changes local configuration and MCP registrations.
- Do not suggest overwriting unrelated or invalid MCP registrations. Worklogger rejects them by design.

## Pick the right path

Use the guided path when the person still needs to choose a provider, Jira board, Bitbucket repositories, or capabilities. Ask them to run:

```shell
npx @santiv343/worklogger
```

Use the manual path only when a reviewed `mcp.json` already exists and the person has selected exact MCP clients. The installer must run in the same environment as the client: PowerShell for Windows applications, or the relevant WSL terminal for Codex or Claude Code inside WSL.

Before any setup, confirm that Node is available with `node --version`. If `npx` still points the `@santiv343` scope at GitHub Packages, ask the person to run `npm config delete @santiv343:registry` once.

## Guide the interactive path

1. Explain the available provider and capability choices using least privilege: enable only what the person needs.
2. Let the person type credentials locally. Do not request them through the conversation.
3. Have them choose a detected MCP client, review the changed file, and confirm.
4. Tell them to restart that client and run `npx @santiv343/worklogger status`.
5. Ask for one read-only check after restart, such as “Show my worklogs for this week” or “List open pull requests in the allowed repository.”

## Guide the manual path

Start from [`config/example.jira-readonly.mcp.json`](../config/example.jira-readonly.mcp.json) for Jira read-only access, or [`config/example.mcp.json`](../config/example.mcp.json) when both providers and broader capabilities are deliberately required. Copy the selected file to a private location, replace the example site, email, board, and scope values, and do not commit it.

The full example enables both providers and write capabilities. Remove every unused provider from both its connection section and `modules`, and retain only the minimum capabilities needed. `mcp.json` never contains a token.

If a secure credential was already saved by the interactive installer, it can be reused. Otherwise, have the person enter a token in the same shell without placing it in shell history. On Bash, Linux, macOS, or WSL:

```shell
read -rsp "Jira API token: " WORKLOGGER_JIRA_API_TOKEN; echo
export WORKLOGGER_JIRA_API_TOKEN
npx --yes @santiv343/worklogger install \
  --config /private/path/mcp.json \
  --clients codex,claude-code \
  --skills \
  --yes
unset WORKLOGGER_JIRA_API_TOKEN
```

`npx --yes` accepts npm's first-install prompt. The final `--yes` is Worklogger's explicit confirmation. `--clients all` targets detected clients; otherwise use one or more of `codex`, `claude-code`, `claude-desktop`, `cursor`, and `windsurf`.

`--skills` is independent from `--clients`: it installs Worklogger's three workflow skills in every compatible assistant destination detected on that user account. Omit it when that wider change is not wanted.

For PowerShell or when handling temporary environment variables is not appropriate, prefer the guided installer so the person enters the credential locally and Worklogger stores it securely.

## Verify and hand off

Ask the person to restart each registered client and run:

```shell
npx @santiv343/worklogger status
```

The status separates local configuration, local server availability, and MCP client registration. A local server does not mean a client is registered. Then ask for one read-only query through the restarted client.

After installation, the person can use plain language. Every Jira or Bitbucket mutation must show a visible preview with actor, target, and exact effect, then obtain explicit confirmation. Never claim a preview exists without showing it to the person.
