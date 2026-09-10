# Custom distributions

## Model

One codebase produces:

- `Community`: editable organization configuration.
- `Managed`: an embedded, immutable organization profile.

Both can include the `jira` MCP add-on, the `bitbucket` MCP add-on, both, or
neither. Selection happens at build time; excluded add-on code is not part of
the resulting binaries.

There is no fork for each organization. Each organization keeps a secret-free
JSON profile outside the repository and builds its installer from a specific
version.

## Profile

Copy `config/example.organization.json` and edit its explicit values. A profile
can define:

- Name, logo, and colors.
- Available modules under `modules`.
- Suggested or allowed Jira sites and boards.
- Allowed Bitbucket workspaces and repositories.
- Maximum MCP capabilities for each provider.
- Operational limits.
- Time Tracking defaults.
- Team report access policy.
- Display and export limits.

Each provider declares `scopeMode`. `restricted` requires allowed resources;
`unrestricted` requires an empty scope and allows selection from resources
visible to the authenticated account. Capabilities and `maximumAllowed*` fields
are maximums: local configuration can only reduce them. Limits without that
prefix are suggested values for new connections.

Profiles must never contain personal emails, API tokens, or other secrets.
A missing module section disables that module for the organization; a present
section does not add code if the binary was built without its add-on.

## Community

```powershell
.\scripts\build-windows.ps1 -Edition Community
```

This includes `configurable-organization`. Users can configure it manually or
import and export validated JSON.

To limit the compiled MCP add-ons:

```powershell
.\scripts\build-windows.ps1 -Edition Community -McpAddons jira
.\scripts\build-windows.ps1 -Edition Community -McpAddons bitbucket
.\scripts\build-windows.ps1 -Edition Community -McpAddons @()
```

Without `-McpAddons`, Jira and Bitbucket are included. This selection does not
enable capabilities by itself; each user configures them within their
permissions afterward.

## Managed

```powershell
.\scripts\build-windows.ps1 `
  -Edition Managed `
  -Profile C:\profiles\company.json `
  -Name Company
```

The build:

1. Checks that the profile exists and contains JSON.
2. Compiles `managed-distribution` without `configurable-organization`.
3. Embeds the profile in the executable.
4. Always validates the embedded schema and, unless `-SkipChecks` is set, runs
   formatting checks, Clippy, and all tests.
5. Generates an NSIS installer and a portable ZIP in `dist/`.

At runtime, Managed does not look for `WORKLOGGER_CONFIG`, a file beside the
`.exe`, or `%APPDATA%\Worklogger\organization.json`. It also omits the UI for
selecting or exporting profiles. This does not affect XLSX or PDF report exports.

The same profile is embedded in the `worklogger-mcp.exe` sidecar. Neither
Desktop nor MCP accepts a replacement through `--profile` or local files.
Desktop currently requires Jira because Time Tracking is its primary
experience. Standalone MCP supports Jira-only, Bitbucket-only, and no-add-on
builds. Reports can be omitted and then disappears from navigation.

## Build both editions

```powershell
.\scripts\build-release-set.ps1 `
  -ManagedProfile C:\profiles\company.json `
  -ManagedName Company
```

The output uses versioned names:

```text
dist/Worklogger-Community-<version>-Setup.exe
dist/Worklogger-Community-<version>-Setup.exe.sha256
dist/Worklogger-Community-<version>-Portable.zip
dist/Worklogger-Community-<version>-Portable.zip.sha256
dist/Worklogger-Company-<version>-Setup.exe
dist/Worklogger-Company-<version>-Setup.exe.sha256
dist/Worklogger-Company-<version>-Portable.zip
dist/Worklogger-Company-<version>-Portable.zip.sha256
dist/Worklogger-Company-<version>-Profile.sha256
```

Each portable ZIP keeps the executable and its assets in one folder. It needs
no installation or administrator permissions, but uses WebView2 Evergreen,
configuration in `%APPDATA%\Worklogger`, and Windows Credential Manager. The
adjacent `.sha256` file can be used to verify the download. Managed also includes
the embedded profile's hash without publishing its contents.

The public repository builds and publishes only Community. An organization
that needs Managed runs the build script in a private pipeline, using a public
tag as its source and an external profile that is not added to the repository
or its workflows. The embedded profile remains the sole source of organization
names, branding, and policy.

## Build machine requirements

- Windows 10 or 11.
- Rust 1.88.
- Dioxus CLI 0.7.9.
- Visual Studio Build Tools 2022 with `Desktop development with C++`.
- NSIS available to Dioxus.

End users need Windows and the installer, not the build tools.

## Before distribution

- Review the profile and check that it contains no secrets.
- Pass tests for both Community and Managed.
- Run the installer and portable app under a clean user account.
- Test initial configuration with a non-administrator account.
- Check that identity and permissions are visible after connecting.
- Apply code signing when a certificate is available.
- Record the installer hash and version.
