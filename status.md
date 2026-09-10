# Operating status

- UTC date: 2026-09-10
- Branch: `feat/audit-hardening`
- Objective: harden the public product after the 0.9.3 release with post-merge CI, opt-in Jira contract verification, clear positioning, and a bounded configurable reporting period.

## In progress

- [-] Submit the verified audit-hardening pull request to `main` and wait for required CI.

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
- Version 0.9.3 is a patch release for the public product surface and distribution path; it does not change provider behavior.
- Provider-facing contract checks remain opt-in: the weekly workflow skips until all six documented repository secrets point to a read-only Jira sandbox.
- Personal report periods default to 7 days, have a hard product ceiling of 31 days, and may be narrowed further by the organization profile. Desktop and MCP persist the same shared preference.
- The Managed Linux MCP CI job enables Jira and Bitbucket explicitly because `managed-distribution` alone intentionally omits those modules and is not a distributable runtime combination.
- Windows CI caught a test-only Desktop fixture import that Linux masked through a development-only path; the import is now `cfg(test)` so production builds remain warning-free on both platforms.

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
- `cargo +1.88.0 check --workspace --all-targets --locked`
- `cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings`
- `cargo +1.88.0 test --workspace --locked` (all passed; 4 read-only live Jira contracts ignored without sandbox credentials)
- `cargo +1.88.0 clippy --package worklogger-mcp --all-targets --no-default-features --features 'jira bitbucket managed-distribution' --locked -- -D warnings`
- `cargo +1.88.0 test --package worklogger-mcp --no-default-features --features 'jira bitbucket managed-distribution' --locked` (113 passed; 3 live Jira contracts ignored without sandbox credentials)
- `npm --prefix packages/setup test` (7 passed)
- `cargo +1.88.0 run --package worklogger-mcp -- --help`
- JSON resources and example configuration parsed successfully; `git diff --check` passed.
- After the Windows CI finding: `cargo +1.88.0 fmt --all --check`, `cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings`, and `cargo +1.88.0 test --workspace --locked` passed again locally.

## Next action

- Create the focused GitHub pull request, wait for its required CI, then merge through the protected `main` branch.
