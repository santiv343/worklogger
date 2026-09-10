# Operating status

- UTC date: 2026-09-10
- Branch: `docs/clarify-public-value`
- Objective: make Worklogger's public GitHub and npm entry points explain its value, safe operating model, and installation path clearly.

## In progress

- [-] Validate the public-product documentation and release workflow, then open a focused pull request.

## Recent decisions

- Manual configuration reuses the local `mcp.json` schema; tokens are accepted only from the secure store or environment variables.
- The TUI remains the guided flow; headless mode requires explicit `--yes` before changing client configuration.
- Public documentation, terminal UI, visible CLI help, confirmations, and user-facing errors use English. Internal resource identifiers remain stable.
- `--skills` is documented separately from `--clients` because it installs into every compatible skill destination detected.
- MCP setup must be an explicit settings checklist, not a one-shot onboarding wizard. A change to one section must not recreate clients or overwrite unrelated provider settings.
- `settings.json` is the canonical secret-free document. Desktop credentials, MCP credentials, MCP grants, clients, and skills remain independently scoped.
- Worklogger uses only its canonical `settings.json`; it does not import or modify previous per-frontend configuration files.
- English and Spanish are a secret-free shared preference. Restarting applies a language change to Desktop and MCP clients.
- Public copy should lead with the outcome for a developer or team, then explain the guarded provider actions and private credential model.
- Public-product audit found no GitHub Release flow, temporary CI artifacts as the only Desktop download path, incomplete GitHub community files, stale Spanish reference docs, and npm metadata that describes an installer instead of the product.
- The public GitHub description and discovery topics now state the concrete product: personal Jira time tracking and reviewed Jira/Bitbucket workflows for AI assistants.
- The public entry points now lead with concrete tasks, supported surfaces, an install path, and the observable confirmation model; implementation detail remains in reference documentation.
- A future `v*` tag will create a GitHub Release containing durable Community Desktop and MCP downloads after the npm publish succeeds.

## Risks

- Unrelated or invalid MCP registrations are never replaced in non-interactive mode.

## Validation

- `cargo +1.88.0 fmt --all --check`
- `cargo +1.88.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.88.0 test --workspace --locked --offline` (live tests without credentials: ignored)
- `npm test` in `packages/setup`
- `cargo +1.88.0 run -p worklogger-mcp -- --help`
- `npm --prefix packages/setup test` (7 passing)
- `node --check packages/setup/index.mjs`
- `node --check packages/setup/platform.mjs`
- `git diff --check`
- Public Markdown scan found no remaining accented Spanish in product documentation.

## Next action

- Push the documentation and release-workflow branch, open a pull request, and let the protected Windows/Linux workflow validate it.
