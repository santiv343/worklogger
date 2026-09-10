# Contributing to Worklogger

Thanks for taking the time to improve Worklogger.

## Before opening an issue

Use the current release and check the [user guide](docs/user-guide/README.md).
For an MCP connection problem, include the client, operating system, Worklogger
version, and the output of `npx @santiv343/worklogger status`. Never include an
API token, personal email address, private repository name, or provider data.

## Before opening a pull request

1. Describe the user problem and the smallest change that solves it.
2. Keep provider credentials, personal settings, and organization profiles out
   of the change.
3. Update public documentation when behavior, setup, or supported clients change.
4. Run the checks relevant to the edited code. The repository workflow is the
   required final verification for supported Windows and Linux targets.

## Local development

Worklogger is written in Rust. Install Rust 1.88 and Dioxus CLI 0.7.9, then run
the checks documented in the workflow before requesting review. The public npm
package is assembled by the release workflow; do not publish a package from a
workstation.

## Pull-request expectations

Keep each pull request focused. Explain what changed, why it is safe, and how it
was verified. Changes to credentials, provider permissions, write confirmations,
or distribution need particular care and review.

## Reporting security issues

Do not open a public issue for a possible vulnerability. Follow
[SECURITY.md](SECURITY.md).
