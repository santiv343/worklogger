# Versioning and releases

## Source of truth

Worklogger uses Semantic Versioning. The canonical version is
`workspace.package.version` in `Cargo.toml`; crates, binaries, and installer
names inherit it. The workflow copies that version into the npm package before
publishing. `packages/setup/package.json` does not maintain a second canonical
version. Every published version must have a `v<major>.<minor>.<patch>` tag on
the exact validated commit.

## What a version means

- `major`: incompatible contracts or persisted data.
- `minor`: new, compatible capabilities.
- `patch`: compatible fixes without new capabilities.

Community and Managed share a version and source code. A Managed distribution
is the reproducible combination of a Worklogger tag and a validated external
profile, not a fork. The artifact must record the distribution name, version,
and profile SHA-256, never a Jira token.

## Published downloads

The tag workflow publishes the public MCP installer package to npm. It also
creates a [GitHub Release](https://github.com/santiv343/worklogger/releases)
with the Community Windows Desktop installer and portable ZIP, Windows and
Linux MCP binaries, and their checksums. GitHub Releases are the durable download
location; Actions artifacts are build outputs with limited retention.

[v0.9.3](https://github.com/santiv343/worklogger/releases/tag/v0.9.3) includes
these public downloads. Changes on `main` are not automatically included in a
published binary; use the matching tag when checking behavior or examples.

Managed profiles and binaries are built and distributed separately through the
organization's private process.

## Release checklist

1. Update `Cargo.toml` and `Cargo.lock`.
2. Update `CHANGELOG.md`.
3. Run formatting checks, Clippy, tests, and builds for Community and Managed.
4. Commit without credentials or accidentally included private profiles.
5. Create and push the `v<version>` tag.
6. Build Windows installers and Windows/Linux MCP binaries from that tag, and
   retain their SHA-256 checksums.
7. Verify that npm and the GitHub Release contain the expected version and that
   release download links resolve to the intended assets.

A distributed binary is immutable. Any rebuild with changes requires a new
version, even when it uses the same organization profile.
