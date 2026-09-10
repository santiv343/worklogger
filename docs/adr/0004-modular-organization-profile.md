# ADR 0004: a shared modular organization profile

## Status

Accepted for incremental implementation. The context and compatibility plan
below record the original migration decision; current user preferences live in
the shared `settings.json`, as described in the
[modular architecture](../architecture/modularity.md).

## Context

When this decision was made, Desktop and MCP maintained separate configuration.
The Desktop profile defined branding and defaults for Jira, Time Tracking, and
Reports, while `mcp.json` repeated scopes, limits, and capabilities alongside
personal account data. This made it difficult to give a team the same setup
without copying accounts or tokens.

Worklogger can also exclude add-ons at build time. A JSON file cannot establish
that a feature is installed or supply code missing from a binary.

## Decision

`organization.json` is the only shareable source of Worklogger configuration.
Its `modules` object contains an optional typed section for each known module.
The following is an abbreviated structure, not a usable profile. The complete,
validatable example is in `config/example.organization.json`:

```json
{
  "schemaVersion": 2,
  "branding": {},
  "modules": {
    "jira": {
      "scopeMode": "unrestricted",
      "sites": [],
      "hours": {},
      "mcpCapabilities": []
    },
    "bitbucket": {
      "scopeMode": "unrestricted",
      "mcpCapabilities": []
    },
    "reports": {}
  }
}
```

The compiled catalog determines which modules are installed. The profile only
permits and configures them. Local preferences can disable modules or reduce
capabilities, but cannot expand the profile. Actual provider permissions remain
the final authority.

Effective availability is the intersection of:

```text
compiled module
∩ section present in organization.json
∩ enabled local preference
∩ authenticated account permission
∩ allowed scope
```

The profile contains no email, discovered identity, token, or session state.
Desktop and the standalone TUI can import the same file. Both create or update
local state with personal selections; secrets are stored separately in the
platform's credential store.

The original migration plan retained `mcp.json` as private local server state
and allowed it to hold resolved values temporarily for compatibility. It was
never the source to share between users. That migration context does not
describe the current persisted settings contract.

## Module semantics

- Missing section: the organization does not configure or offer that module.
- Present section and compiled add-on: the module can be enabled.
- Present section without the compiled add-on: the module is reported as not
  installed and does not run.
- Compiled add-on without a section: it stays hidden or awaits configuration,
  depending on the edition.
- A module unknown to the installed version is rejected; it is not run or
  partially interpreted.

Scope is always explicit. `scopeMode: "restricted"` requires at least one
allowed Jira site or Bitbucket workspace. `scopeMode: "unrestricted"` requires
an empty list and deliberately allows selection of any resource visible to the
provider account. An empty scope is never interpreted by inference.

The profile's `mcpCapabilities` and `maximumAllowed*` fields are organization
maximums; other limits are defaults. Setup presents that permitted subset and
each user chooses which capabilities to enable locally. It does not
automatically enable every permitted write.

Jira is one module. Issues and Time Tracking are internal capability groups,
with Time Tracking nested under `modules.jira`. Reports consumes reads and
does not grant Jira permissions. Bitbucket is an independent module.

## Distributions

Community allows users to import, edit, and export the secret-free profile.
Managed embeds the same schema in both Desktop and its MCP sidecar and does not
allow replacement at runtime. Custom binaries are built from an external
profile; the generic repository contains no organization-specific data.

Other products maintain their own profiles and contexts. If a future
distribution needs to deliver several products through one file, a distribution
envelope can reference their profiles. Schemas, credentials, and domains must
not be mixed into Worklogger.

## Compatibility

The original compatibility plan accepted flat schema `1` and migrated it in
memory, with new exports using schema `2`. Schema, module, or limit errors must
fail before modifying existing configuration or credentials. This historical
plan does not imply that current releases import old per-frontend settings.

## Consequences

- A file without personal data can reproduce branding, modules, scopes,
  limits, and capabilities across Desktop and MCP.
- Adding a module requires its type, validation, build feature, and interface
  adapter. A JSON key alone does not enable code.
- Local policy is not a cryptographic boundary. Future centralized enforcement
  will require a signed profile or an authorization service.
