# Modular architecture

## Purpose

Support small, configurable distributions without duplicating business logic
across Desktop, CLI, MCP, or a future web interface. Add-ons are static and known
at build time; availability and exposure are decided at runtime.

## Dependency map

```text
Desktop · CLI · MCP · future web API
                 │
                 ▼
           typed use cases
      ┌──────────┼───────────┐
      ▼          ▼           ▼
 Jira module   Reports   Bitbucket module
  ├─ Issues
  └─ Time Tracking
      │          │           │
      └──────────┴── ports ──┘
                         │
                         ▼
       Jira Cloud · Bitbucket Cloud · keyring · filesystem
```

`Jira` is one installable module. `Issues` and `Time Tracking` are internal
capability groups: they share a connection, authenticated identity, board, and
policy, while their use cases remain separate. `Bitbucket` is independent.
`Reports` consumes enabled reads and never expands provider permissions.

Interfaces never call REST clients directly. Each use case defines the smallest
port it needs and receives an adapter at the composition root.

## Standalone MCP

MCP ships as its own binary, without WebView2 or a dependency on Desktop.
`worklogger-mcp` reuses domain, use-case, and adapter crates and adds the MCP
`stdio` transport and its own composition root.

The same executable handles initial configuration and stores tokens separately
from settings, using Windows Credential Manager or a protected Linux store.
The effective tool list follows compiled features, the organization profile,
user configuration, and provider permissions. Desktop is never required to use
MCP.

Time Tracking queries the authenticated account's worklogs. Issue and
pull-request mutations require a preview and explicit confirmation. Other
people's worklogs remain read-only when reporting access is authorized.

### Installation and configuration

With no arguments, the binary opens the main TUI with status, paths, modules,
and detected clients. Commands such as `serve`, `setup`, `clients`, `status`,
and `uninstall` use the same underlying use cases. `setup` accepts
`--profile <organization.json>` to install a shared profile. Client registration
shows its target and asks for confirmation before making changes.

There are two entry points to the same use case:

1. Desktop exposes installation, removal, and available modules and capabilities
   through **Configuration → MCP**.
2. `npx @santiv343/worklogger` runs the bundled binary, which is installed in a
   stable user location and opens the TUI. Node is only the bootstrap; the
   server does not depend on it.

Installation is idempotent: running it again updates or repairs the existing
registration instead of duplicating it. A prior version's registration remains
owned by Worklogger even if its binary is missing, and can be repaired or
removed. An entry with the same name from another source remains a conflict.
The binary location depends on the user and operating system. Tokens are never
written to MCP client configuration. Desktop and the TUI use a versioned
per-user location, so moving a portable app does not break existing registrations.

The original client catalog included Codex, Claude Code, Claude Desktop, Cursor,
and Windsurf. See the [client support guide](../mcp-client-support.md) for the
current catalog and configuration targets. Codex registration uses its official
CLI. On Windows, the standard npm installation is resolved from `codex.cmd` to
Node and `codex.js` without executing the shim through a shell. JSON clients
receive only the `mcpServers.worklogger` entry; other keys are preserved. Writes
are atomic, reject symbolic links, and check that the document has not changed
before writing. Invalid configuration fails closed without being replaced.
Client processes have a configurable timeout, and Desktop queries them outside
the UI thread.

Module settings allow users to reduce capabilities within each module. Jira's
Time Tracking and Issues appear as groups, never independent add-ons.
Configuration cannot expand compiled functionality or effective permissions.
For example, enabling Jira Time Tracking does not grant access to other
people's hours or make a team report writable.

## Contexts

The boundaries below describe architectural responsibilities; they are not a
catalog of every tool currently exposed by MCP.

| Context | Responsibility | Does not know about |
| --- | --- | --- |
| Platform Core | Add-ons, capabilities, configuration, scopes | Jira, HTTP, Dioxus |
| Jira / connection | Account, boards, shared scope and permissions | Time Tracking rules, UI |
| Jira / Issues | Issues, fields, comments, transitions | Time Tracking rules, UI |
| Jira / Time Tracking | Ranges, durations, personal worklogs, summaries | Dioxus |
| Reports | Filters, aggregates, personal and team read models | Mutations, tokens |
| Bitbucket | Repositories, PRs, reviews, tasks, pipelines | Jira, Time Tracking |
| Work Hub | Cross-provider queries and explainable signals | Concrete HTTP clients |
| Interfaces | Interaction, presentation, confirmation | Business rules |

There is no universal project-management provider. Only stable concepts are
shared: identity, ranges, durations, and external references.

## Static add-on descriptor

```text
AddonDescriptor
- ID and version
- translation namespace
- provided and required capabilities
- configuration namespace
- supported interfaces
```

Descriptors are registered manually under Cargo features. There is no plugin
ABI, DLL loading, code download, or marketplace.

## Capabilities

Capabilities are more specific than module-level `read/write`. The architecture
uses names such as these; availability depends on the implemented catalog:

```text
jira.identity.read
jira.issue.read
jira.issue.comment
jira.issue.transition
jira.hours.read.self
jira.hours.write.self
time-entry.read.team
report.hours.personal
report.hours.team
bitbucket.pr.read
bitbucket.pr.review
bitbucket.pipeline.read
```

Effective availability is the intersection of:

```text
compiled
∩ allowed by the organization
∩ enabled by the user
∩ authorized by the provider
∩ valid for the scope
∩ exposed through the current interface
```

## Configuration layers

1. `BuiltInDefaults`: safe defaults and neutral branding.
2. `OrganizationProfile`: branding, sites, allowed add-ons, limits, policies.
3. `UserPreferences`: connection, scope, targets, display preferences.
4. `SecretStore`: provider tokens; JSON contains only references.
5. `SessionState`: identity, permissions, cache; never a persistent authority.

`OrganizationProfile` is serialized as one `organization.json` with an
optional section per module. Desktop, TUI, and MCP share the type and file.
User preferences are stored only in the shared `settings.json`; isolated
headless overrides remain separate and do not modify it. A JSON file cannot
install add-ons: it only configures or narrows what the binary already includes.

A locally editable JSON file cannot grant sensitive access. Strong central
policy would require a signature or an authorization service. Effective provider
permissions and fail-closed behavior remain mandatory.

## Compile time and runtime

Compile-time features remove code and dependencies:

```text
hours
reports
configurable-organization
managed-distribution
mcp-management
worklogger-mcp/jira
worklogger-mcp/bitbucket
```

Runtime configuration can only reduce compiled functionality. The distribution
plan calls for a small number of tested combinations, such as Community,
Managed/PM, and Developer where needed, rather than a `2^N` variant matrix.

## Incremental evolution

The original migration sequence and recorded milestones are preserved below.
These describe architectural progress, not release availability; see the
[changelog](../../CHANGELOG.md) for shipped behavior.

1. Register existing add-ons without changing behavior. Completed.
2. Extract UI orchestration into use cases. In progress in the migration plan.
3. Add ports to Time Tracking. Completed.
4. Separate Jira transport while adding general CRUD. Completed in MCP.
5. Extract reusable Reports analytics and renderers. Planned.
6. Introduce Bitbucket as an independent module. Completed in MCP.
7. Expose the same use cases through MCP, CLI, or web. In progress in the plan.
