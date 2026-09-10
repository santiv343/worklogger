# Security policy

## Supported versions

Security fixes are applied to the current released version of Worklogger.

## Reporting a vulnerability

Please do not disclose vulnerabilities in a public issue. Use GitHub's private
security-advisory flow for this repository, or contact the repository owner
through GitHub with a concise report.

Include the affected version, environment, reproduction steps, impact, and any
suggested mitigation. Do not include provider tokens, private Jira or Bitbucket
data, or another person's personal information.

## Security boundaries

Worklogger stores credentials locally and keeps them out of generated settings
and MCP client configuration. Provider permissions, configured scopes, and
explicit confirmation are separate controls. See the
[permission and privacy model](docs/security/permission-model.md) for details.

## Dependency hygiene

Dependabot checks Cargo and npm dependencies weekly. Review advisories against
the shipped targets before acting: a package listed in `Cargo.lock` can be
limited to an unsupported platform or feature and may not be present in a
released artifact. Record that evidence when dismissing an alert.
