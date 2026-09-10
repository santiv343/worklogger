# ADR 0003: MCP mutations that preserve provider semantics

Date: 2026-09-03
Status: accepted

## Context

ADR 0002 initially limited Bitbucket to read-only access. The MCP server also
needs to support everyday Jira issue and pull-request changes without inventing
a universal API or coupling the two providers.

## Decision

- Jira remains one module with internal Issues and Time Tracking groups.
- Bitbucket is an independent module that can be excluded from a build.
- Each mutation retains the provider's model and requires a specific capability.
- Inputs never choose the actor; operations use the authenticated account.
- Every mutation requires a preview and a single-use confirmation token.
- Pull-request confirmation is bound to the exact commits, branches,
  participants, and revision observed. These are compared again immediately
  before writing.
- Jira field-edit confirmation is bound to the current values of the affected
  fields. Those fields are read again before writing.
- The Jira board and allowed Bitbucket repositories form explicit boundaries
  that fail closed.
- Write capabilities depend on their corresponding read capabilities, so a
  preview cannot become a way to bypass read permissions.

## Consequences

Bitbucket is no longer read-only within MCP, but does not become a dependency of
Time Tracking or Jira. Work Hub can still use it as read-only evidence. Each
distribution selects its modules through Cargo features; runtime configuration
can only reduce the available capabilities.

Bitbucket Cloud and Jira Cloud do not document a uniform version precondition
for these writes. Worklogger compares the confirmed snapshot immediately before
sending a mutation; the provider performs the final atomic validation and may
return a conflict. This remaining limitation is accepted because there is no
public compare-and-swap operation that Worklogger can apply uniformly.
