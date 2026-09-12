# MCP client support

Worklogger installs one local `worklogger-mcp serve` command. It never copies a
provider token into a client configuration file. The installer only offers a
client when its documented user configuration directory already exists.

## Common JSON adapter

These clients use the same JSON `mcpServers` adapter. Adding another compatible
client requires only its documented user-level JSON destination, not another
registration implementation.

| Client | Install value | Configuration |
| --- | --- | --- |
| Claude Code | `claude-code` | `~/.claude.json` |
| Claude Desktop | `claude-desktop` | Native app configuration |
| Cursor | `cursor` | `~/.cursor/mcp.json` |
| Windsurf | `windsurf` | `~/.codeium/windsurf/mcp_config.json` |
| Qwen Code | `qwen-code` | `~/.qwen/settings.json` |
| Gemini CLI | `gemini-cli` | `~/.gemini/settings.json` |
| Kiro | `kiro` | `~/.kiro/settings/mcp.json` |
| GitHub Copilot CLI | `github-copilot` | `~/.copilot/mcp-config.json` |

The common MCP stdio shape is:

```json
{
  "mcpServers": {
    "worklogger": {
      "command": "/absolute/path/to/worklogger-mcp",
      "args": ["serve"]
    }
  }
}
```

## Special adapters

| Client | Install value | Configuration | Why it is special |
| --- | --- | --- | --- |
| Codex | `codex` | Codex CLI | Its CLI is the supported registration interface. |
| Trae Code CLI | `trae-code` | `~/.trae/traecli.toml` | Its documented user configuration is TOML. |

Worklogger preserves the Trae Code CLI document and writes an equivalent
`[mcp_servers.worklogger]` table.

Use the guided client screen, or select exact clients without interaction:

```shell
npx --yes @santiv343/worklogger install \
  --config /private/path/mcp.json \
  --clients qwen-code,gemini-cli,kiro,github-copilot,trae-code \
  --yes
```

`--clients all` selects every detected automatic client. The installer refuses
to overwrite another server's `worklogger` entry, retains unrelated entries,
and shows its registration status after the operation.

## Project-scoped clients

Trae IDE supports MCP configuration in `.trae/mcp.json` inside a trusted
project. Worklogger does not choose or create a project on a person's behalf,
so it deliberately does not register Trae IDE automatically. Use the MCP
client UI in Trae IDE to import the standard JSON entry above after selecting
the intended project.

## Skills

MCP registration and assistant skills are independent. `npx
@santiv343/worklogger skills` installs only the three Worklogger-owned
`SKILL.md` workflows and verifies their state. It supports the generic Agent
Skills location as well as Codex, Claude Code, and Windsurf destinations; it
never replaces a similarly named directory it did not create. This is the same
portable `SKILL.md` convention used by the Vercel Agent Skills ecosystem, but
it does not assume that every MCP client also loads skills.

## Provider references

- [Qwen Code MCP configuration](https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/)
- [Gemini CLI MCP setup](https://geminicli.com/docs/cli/tutorials/mcp-setup/)
- [Kiro MCP configuration](https://kiro.dev/docs/mcp/configuration/)
- [GitHub Copilot CLI MCP configuration](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
- [Trae Code CLI configuration](https://docs.trae.cn/cli_config-file)
- [Trae IDE MCP servers](https://docs.trae.cn/ide_add-mcp-servers)
- [Vercel Agent Skills](https://vercel.com/docs/agent-resources/skills)
