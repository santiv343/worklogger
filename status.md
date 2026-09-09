# Operating status

- UTC date: 2026-09-09
- Branch: `main`
- Objective: provide documented manual and non-interactive MCP installation for people and assistants, with an English-only interface.

## In progress

- [x] Translated the complete terminal interface and visible CLI help to English; validated release `0.7.10`.

## Recent decisions

- Manual configuration reuses the local `mcp.json` schema; tokens are accepted only from the secure store or environment variables.
- The TUI remains the guided flow; headless mode requires explicit `--yes` before changing client configuration.
- Public documentation, terminal UI, visible CLI help, confirmations, and user-facing errors use English. Internal resource identifiers remain stable.
- `--skills` is documented separately from `--clients` because it installs into every compatible skill destination detected.

## Risks

- Unrelated or invalid MCP registrations are never replaced in non-interactive mode.

## Validation

- `cargo +1.88.0 fmt --all --check`
- `cargo +1.88.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.88.0 test --workspace --locked --offline` (live tests without credentials: ignored)
- `npm test` in `packages/setup`
- `cargo +1.88.0 run -p worklogger-mcp -- --help`

## Next action

- Publish the `v0.7.10` tag and verify the npm package.
