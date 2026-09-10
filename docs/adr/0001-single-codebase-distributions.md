# ADR 0001: one codebase, multiple distributions

Date: 2026-09-03
Status: accepted

## Context

Worklogger needs a configurable generic edition and organization editions whose
profiles cannot be changed. Maintaining a repository or fork for each
organization would duplicate fixes, security work, tests, and releases.

## Decision

Maintain one generic codebase.

- Community builds include `configurable-organization`.
- Managed builds include `managed-distribution` and an external profile supplied
  through `WORKLOGGER_DISTRIBUTION_PROFILE`.
- The build copies the validated profile into the generated Cargo artifact.
- Managed builds omit the profile import/export UI and do not read overrides.
- Real organization profiles stay outside the generic repository.

## Benefits

- Fixes reach every organization.
- The base product contains no organization-specific data.
- Installers are reproducible.
- Profile restrictions do not depend on hiding buttons.
- Future editions can select add-ons through Cargo features.

## Costs and limitations

- Every published combination must be tested explicitly.
- Installer branding remains shared until metadata and icon generation support
  individual distributions.
- A Managed binary restricts configuration; it does not replace provider
  permissions or a signed central policy.
- Code signing remains a separate process.

## Alternatives rejected

- A repository or fork for each organization.
- An editable JSON file distributed alongside the installer.
- Downloading profiles from a service before validating the product's value.
- A dynamic plugin system or marketplace.
