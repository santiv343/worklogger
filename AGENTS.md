# Working on Worklogger

Read the README and the relevant guides in `docs/` before changing behavior.
These instructions complement any applicable workspace rules.

## Scope and continuity

- Keep public code, documentation, examples, and comments in English. Use the
  owner's language in conversation.
- Start with the user problem, the smallest useful change, and its verification.
  Distinguish planned, implemented, tested, and blocked work.
- Keep private plans and review receipts under ignored `.local/`. If a local
  `.local/status.md` exists, read and update its objective, one active task,
  accepted decisions, evidence, blockers, and next action. A fresh checkout
  must remain usable without that file.
- Record durable decisions in ADRs and update the relevant public guide when
  behavior or an architectural boundary changes. Do not publish session logs.
- Preserve other people's changes and keep unrelated fixes out of scope.
- Report meaningful progress during extended work. Do not call work complete
  while required checks are pending, and do not hide warnings or assumptions.
- When asked to continue, execute the recorded next action. Explain the scope,
  consequences, and deferred work of material decisions.

## Product boundaries

- Keep the product generic. Organization names, sites, boards, fields, branding,
  and rules belong in external profiles or discovered provider data.
- Managed builds prevent organizational configuration changes. Community builds
  allow manual configuration and JSON import.
- Capabilities remain optional and removable at build time where dependencies
  allow. Desktop and MCP already ship; interfaces reuse the same business logic.
- Identity comes from the authenticated account. Only its own worklogs may be
  changed. Other people's hours are read-only when explicitly authorized.
- Sensitive writes show the actor, target, and effect before confirmation.
- Secrets belong in the platform credential store, never JSON, source, docs,
  logs, reports, or conversation.
- Commits, pull requests, and meetings may inform a person's decision; never
  derive and submit worklogs automatically from them.
- Prefer a useful, verifiable slice. Reuse existing code before adding a
  dependency, service, or abstraction. Validate real use before generalizing.
- Obtain independent review for architecture, security, and significant releases;
  record objections as well as conclusions.
- Real-account checks are read-only unless the owner explicitly authorizes a
  mutation. Automated tests use doubles or isolated servers.

## User experience and distribution

- Top-level navigation represents modules such as Jira and Reports. Hours is
  part of Jira, not a separate module.
- Configuration is a global modal with navigation for General and each module.
- Partial refreshes use skeletons only for the affected data.
- Compact actions use recognizable icons and tooltips. Keep visible labels for
  actions whose purpose is not obvious, such as export.
- Board, issue, and date selectors support search when needed, dismiss on outside
  clicks, and reject future worklog dates.
- Date presets keep stable meanings; navigation uses the selected preset's unit,
  and labels describe the actual period.
- Onboarding verifies credentials before discovering and selecting resources.
- Personal and team reports remain separate. Team views require explicit access
  and never enable changes to another person's worklogs.
- Team filters include the scoped collaborators, including people with zero hours.
- Exports represent the full view and include traceable raw data. Do not advertise
  an unavailable or disabled format.
- End-user Desktop runs without Node, Rust, or development commands. Preserve
  hot reload and a direct way to see development changes.

## Architecture and implementation

- Ports belong to their consuming use cases. The domain does not import UI,
  HTTP, or provider adapters.
- Normalize concepts only after a real use case proves they are shared.
- Keep identity, permissions, and references within their connection namespace.
- A visible UI capability never replaces authorization or ownership checks.
- Partial errors retain their origin and traceability. Prefer tested vertical
  slices to broad horizontal migrations.
- Begin with a failing test for new behavior or a risky refactor.
- Maintain one source of truth: tool identifiers in their catalog, modules and
  permissions in enums, defaults and limits in typed configuration, and visible
  text in language resources. Consumers and tests derive shared values from
  these sources; incidental literals need no abstraction.
- Keep new functions under 20 lines. Do not add speculative interfaces.
- Run formatting, Clippy, tests, and feature checks relevant to the change.
  The [contributor guide](CONTRIBUTING.md) and CI workflows define validation.
