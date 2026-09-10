# ADR 0002: provider neutrality through vertical slices

Date: 2026-09-03
Status: accepted; Bitbucket scope updated by ADR 0003

## Context

Worklogger should be able to support Jira, Notion, source-control providers,
and custom systems. At the time of this decision, the Time Tracking domain,
session, and reports still use Jira concepts and DTOs. Normalizing all of them
at once would affect much of the application and assume equivalences that have
not been demonstrated.

Two independent reviews agreed that Jira and Bitbucket demonstrate how to
compose different provider roles. They do not establish that Jira and Notion
are interchangeable task sources or worklog destinations.

## Decision

- Treat provider neutrality as a dependency rule.
- Define narrow ports owned by each consuming use case.
- Migrate one complete workflow at a time, starting with personal time tracking.
- Keep Jira as the first source and destination for worklogs.
- Introduce Bitbucket first as optional, read-only evidence. Later mutations
  that follow the provider's model are governed by ADR 0003.
- Validate an abstraction against a second real implementation before declaring
  it stable or public.

The first planned ports cover reading personal hours, changing personal
worklogs, reading candidate tasks, and reading evidence when Bitbucket is
available. Identity and capabilities belong to an authenticated connection;
they are not independent universal services.

## Boundaries

- There is no universal project-management model.
- Statuses, transitions, rich comments, pull requests, and pipelines retain
  their provider semantics.
- References include the connection and an opaque identifier.
- A worklog destination accepts only compatible references.
- A cached capability can guide the UI but cannot authorize a mutation.
- This stage does not introduce hosted storage, a marketplace, or a plugin ABI.

## Consequences

The migration is incremental and keeps the application working. For a time,
provider-neutral personal models and Jira-specific team reports will coexist.
That temporary duplication is preferable to a broad rewrite that risks
ownership checks, exports, or permissions.

While configuration permits only one connection per site, the first adapter
uses the Jira site origin as a stable namespace. Supporting two connections to
the same site will require a persisted connection identifier and migration of
existing references. Emails and tokens must not be concatenated into identifiers.

Notion will initially be evaluated as a read-only source with explicit property
mapping, and only when there is demand. Hosting worklogs would require
authentication, synchronization, auditing, backups, and retention; it is outside
the next MVP's scope.

## Validation

The design is considered validated when:

1. The personal workflow runs through a port and an in-memory adapter.
2. Jira implements that port without losing partial errors or ownership checks.
3. Bitbucket can add evidence without becoming a required dependency.
4. A second real source challenges or confirms the contracts before publication.
