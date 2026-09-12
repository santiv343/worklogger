# Changelog

Notable Worklogger changes are documented in this file. The project uses
Semantic Versioning and dates use the `YYYY-MM-DD` format.

## [0.9.3] - 2026-09-10

### Added

- GitHub Releases now publish the Community Windows installer, portable ZIP,
  Windows and Linux MCP binaries, and checksums from a successful version tag.
- Public contribution, security, and issue-reporting guidance.

### Changed

- GitHub and npm now describe Worklogger through personal time tracking and
  reviewed Jira and Bitbucket workflows instead of installer internals.
- Public product, architecture, and security documentation is available in
  English with clear installation, privacy, and release paths.

## [0.9.2] - 2026-09-10

### Fixed

- Changing the interface language now takes effect immediately in the running
  TUI and persists for the next launch.
- Back navigation now returns one screen at a time from Settings subflows.
- TUI confirmations, keyboard hints, and result messages consistently follow
  the selected language; action results are no longer deferred until later
  navigation.
- Skill installation no longer shows a redundant completion dialog after its
  verified status screen.

## [0.9.1] - 2026-09-10

### Fixed

- The TUI dashboard now shows a compact MCP-client preview and a visible link
  to the complete client list instead of silently clipping additional clients
  in a standard terminal.

## [0.9.0] - 2026-09-10

### Added

- Automatic MCP registration for Qwen Code, Gemini CLI, Kiro, and GitHub
  Copilot through their documented JSON `mcpServers` configuration.
- Automatic MCP registration for Trae Code CLI through its documented TOML
  configuration, preserving existing comments and unrelated servers.
- An MCP client compatibility guide that separates common JSON clients from
  special adapters and documents Trae IDE's project-scoped setup.

## [0.8.0] - 2026-09-09

### Added

- Desktop and MCP now use one secret-free `settings.json` document for shared
  provider preferences, scopes, limits, and the selected interface language.
- Direct hierarchical Jira and Bitbucket settings sections in the MCP TUI,
  including connection, scope, permissions, defaults, and advanced limits.
- English and Spanish interface resources. The chosen language is shared by
  Desktop and MCP and takes effect after restart.

### Changed

- Desktop and MCP preserve each other's settings and MCP consent boundaries.
- Worklogger no longer imports or modifies previous per-frontend settings
  files; a new installation starts from the canonical shared document.

## [0.7.10] - 2026-09-09

### Changed

- The complete terminal interface, confirmations, status messages, CLI help,
  and user-facing MCP errors now use English.

## [0.7.9] - 2026-09-09

### Added

- `install --config ... --clients ... --yes` installs a reviewed local MCP
  configuration without opening the TUI, optionally installs workflow skills,
  and refuses unrelated or invalid client registrations.
- A minimal Jira read-only configuration example and an assistant-specific
  installation guide for human-guided setup.

### Changed

- Public README files, user guide, npm package guide, and walkthrough diagrams
  are available in English and place MCP installation before architecture notes.
- The local-server dashboard label now describes server availability instead of
  implying that a full client installation has occurred.

## [0.7.8] - 2026-09-09

### Changed

- The TUI uses panels with contextual navigation, visible focus, and a consistent
  visual hierarchy across interactive flows.
- Skills shows one card per assistant with installation, update, and conflict
  counts before and after the workflow.
- Selectors, confirmations, fields, messages, and progress states use compact
  layouts that adapt to their content and the terminal size.

## [0.7.7] - 2026-09-09

### Changed

- The TUI shows progress, results, and errors within the same session instead of
  returning messages to the console after a workflow ends.
- The dashboard removes nested panels and simplifies the visual hierarchy for
  status, actions, and focus.
- Skill installation first shows each assistant's status, detects conflicts
  without overwriting unrelated files, and verifies the final result.

## [0.7.6] - 2026-09-09

### Changed

- The TUI keeps one visual session when navigating between screens; returning
  with `Esc` no longer exposes the console between steps.
- Bitbucket settings can retain additional reviewers and a source-branch
  closure preference for future pull requests.

### Fixed

- An asynchronous merge reports that it is still pending and includes the task
  identifier instead of reporting completion.
- Desktop preserves the time, time zone, and seconds when editing a worklog
  without changing those fields.
- Fixed waits for client processes, partial skill installation, report rosters,
  and propagation of Clippy errors on Windows.

## [0.7.5] - 2026-09-09

### Changed

- Settings presents Jira and Bitbucket as descriptive cards and groups each
  integration's capabilities into a single selection.
- Text fields support editing default values, moving the cursor, and pasting;
  secrets remain masked.
- Lists use a consistent visual hierarchy, focus color, and keyboard or mouse
  navigation.

### Fixed

- `Esc` and `q` cancel the current step without accidentally choosing another
  option; returning from the main menu preserves the dashboard.
- Clicking outside a list or inside a scrolled list no longer selects the wrong
  item.

## [0.7.4] - 2026-09-09

### Changed

- The entire MCP setup flow uses the TUI: providers, sites, boards, repositories,
  capabilities, clients, confirmations, and text fields.
- Tokens are entered in a masked field within the TUI.

## [0.7.3] - 2026-09-09

### Changed

- The interactive menu uses a TUI with panels, colors, keyboard navigation, and
  mouse selection.

## [0.7.2] - 2026-09-09

### Fixed

- npm publication explicitly configures authentication for the public registry
  using the CI secret.

## [0.7.1] - 2026-09-09

### Fixed

- The npm package is validated and published independently of the Desktop
  installer.
- The Codex status fixture is portable across Unix shells in CI.

## [0.7.0] - 2026-09-09

### Added

- Public MCP bootstrap and installable workflow skills without a Worklogger
  account or token.
- MIT license for the source code and npm package.

### Changed

- The public pipeline distributes only the Community edition. Managed profiles
  are built separately with external configuration.
