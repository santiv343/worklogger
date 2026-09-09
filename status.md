# Operating status

- UTC date: 2026-09-09
- Branch: `main`
- Objective: make Desktop and MCP two frontends over one documented, secret-free settings domain, with direct English and Spanish settings experiences.

## In progress

- [-] Complete the shared settings domain, direct hierarchical settings UI, and Desktop/MCP adapters without weakening credential or MCP-consent boundaries.

## Recent decisions

- Manual configuration reuses the local `mcp.json` schema; tokens are accepted only from the secure store or environment variables.
- The TUI remains the guided flow; headless mode requires explicit `--yes` before changing client configuration.
- Public documentation, terminal UI, visible CLI help, confirmations, and user-facing errors use English. Internal resource identifiers remain stable.
- `--skills` is documented separately from `--clients` because it installs into every compatible skill destination detected.
- MCP setup must be an explicit settings checklist, not a one-shot onboarding wizard. A change to one section must not recreate clients or overwrite unrelated provider settings.
- `settings.json` is the canonical secret-free document. Desktop credentials, MCP credentials, MCP grants, clients, and skills remain independently scoped.
- Worklogger uses only its canonical `settings.json`; it does not import or modify previous per-frontend configuration files.
- English and Spanish are a secret-free shared preference. Restarting applies a language change to Desktop and MCP clients.

## Risks

- Unrelated or invalid MCP registrations are never replaced in non-interactive mode.

## Validation

- `cargo +1.88.0 fmt --all --check`
- `cargo +1.88.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.88.0 test --workspace --locked --offline` (live tests without credentials: ignored)
- `npm test` in `packages/setup`
- `cargo +1.88.0 run -p worklogger-mcp -- --help`

## Next action

- Finish the remaining direct setup paths, run the full validation matrix, complete the repository-wide English cleanup, and publish.
