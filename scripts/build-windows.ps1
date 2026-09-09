[CmdletBinding()]
param(
    [ValidateSet('Community', 'Managed')]
    [string]$Edition = 'Community',
    [string]$Profile,
    [string]$Name,
    [string]$OutputDirectory,
    [ValidateSet('jira', 'bitbucket')]
    [string[]]$McpAddons = @('jira', 'bitbucket'),
    [switch]$SkipChecks
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$WindowsRustTarget = 'x86_64-pc-windows-msvc'
$DioxusAppRelativePath = 'target\dx\worklogger-desktop\release\windows\app'
$PortableExecutableName = 'Worklogger.exe'
$McpExecutableName = 'worklogger-mcp.exe'
$McpSidecarStem = 'target\worklogger-sidecars\worklogger-mcp'
$PortableInstructionsRelativePath = 'distribution\windows\LEEME-PORTABLE.txt'

function Get-RepositoryRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
}

function Add-UserToolPaths {
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if ((Test-Path $cargoBin) -and -not $env:Path.Contains($cargoBin)) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

function Get-VisualStudioDeveloperCommand {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) {
        throw 'Falta Visual Studio Build Tools 2022 con el workload Desktop development with C++.'
    }
    $installation = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ([string]::IsNullOrWhiteSpace($installation)) {
        throw 'Falta el workload Desktop development with C++ de Visual Studio Build Tools.'
    }
    return Join-Path $installation 'Common7\Tools\VsDevCmd.bat'
}

function Import-VisualStudioEnvironment {
    $developerCommand = Get-VisualStudioDeveloperCommand
    $environmentLines = & $env:ComSpec /s /c "`"$developerCommand`" -no_logo -arch=x64 && set"
    foreach ($environmentLine in $environmentLines) {
        $separator = $environmentLine.IndexOf('=')
        if ($separator -le 0) { continue }
        $variableName = $environmentLine.Substring(0, $separator)
        $variableValue = $environmentLine.Substring($separator + 1)
        Set-Item -Path "Env:$variableName" -Value $variableValue
    }
}

function Assert-BuildTools {
    foreach ($command in @('cargo', 'dx', 'link')) {
        if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
            throw "No se encontró $command en PATH. Revisá los requisitos de distribución."
        }
    }
}

function Invoke-Checked([scriptblock]$Command, [string]$FailureMessage) {
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$FailureMessage Código de salida: $LASTEXITCODE."
    }
}

function Get-ManagedProfile([string]$RepositoryRoot) {
    if ([string]::IsNullOrWhiteSpace($Profile)) {
        throw 'Managed requiere -Profile con un JSON de organización.'
    }
    $resolved = (Resolve-Path $Profile).Path
    $document = Get-Content -Raw -Path $resolved | ConvertFrom-Json
    if ([string]::IsNullOrWhiteSpace($document.branding.companyName)) {
        throw 'El perfil no contiene branding.companyName.'
    }
    return @{ Path = $resolved; Company = [string]$document.branding.companyName }
}

function Get-Features([string]$SelectedEdition, [string[]]$SelectedMcpAddons) {
    $features = @('hours', 'reports')
    if ($SelectedEdition -eq 'Managed') {
        $features += 'managed-distribution'
    } else {
        $features += 'configurable-organization'
    }
    $features += $SelectedMcpAddons | ForEach-Object { "mcp-$_" }
    return $features -join ' '
}

function Invoke-McpBuild(
    [string]$RepositoryRoot,
    [string]$RustTarget,
    [string[]]$SelectedMcpAddons,
    [string]$SelectedEdition
) {
    $mcpFeatureList = @($SelectedMcpAddons)
    if ($SelectedEdition -eq 'Managed') {
        $mcpFeatureList += 'managed-distribution'
    }
    $mcpFeatures = $mcpFeatureList -join ','
    Invoke-Checked {
        if ($mcpFeatures) {
            cargo build --package worklogger-mcp --release --target $RustTarget --locked `
                --no-default-features --features $mcpFeatures
        } else {
            cargo build --package worklogger-mcp --release --target $RustTarget --locked `
                --no-default-features
        }
    } 'Falló la compilación de Worklogger MCP.'
    $source = Join-Path $RepositoryRoot "target\$RustTarget\release\$McpExecutableName"
    $sidecar = Join-Path $RepositoryRoot "$McpSidecarStem-$RustTarget.exe"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $sidecar) | Out-Null
    Copy-Item -Force -Path $source -Destination $sidecar
}

function Get-Version([string]$RepositoryRoot) {
    $manifest = Get-Content -Raw -Path (Join-Path $RepositoryRoot 'Cargo.toml')
    $match = [regex]::Match($manifest, 'version\s*=\s*"([^"]+)"')
    if (-not $match.Success) {
        throw 'No se pudo determinar la versión del workspace.'
    }
    return $match.Groups[1].Value
}

function Get-SafeName([string]$Value) {
    $safeName = ($Value.Trim() -replace '[^A-Za-z0-9._-]+', '-').Trim('-')
    if ([string]::IsNullOrWhiteSpace($safeName) -or $safeName -match '^\.+$') {
        throw 'El nombre de la distribución no es válido.'
    }
    return $safeName
}

function Get-StagingDirectory([string]$RepositoryRoot, [string]$SafeName) {
    $distributionRoot = Join-Path $RepositoryRoot 'target\distribution'
    $stagingDirectory = Join-Path $distributionRoot $SafeName
    $expectedParent = [IO.Path]::GetFullPath($distributionRoot).TrimEnd('\')
    $actualParent = [IO.Directory]::GetParent([IO.Path]::GetFullPath($stagingDirectory)).FullName
    if ($actualParent -ne $expectedParent) {
        throw 'El directorio de staging no es seguro.'
    }
    return $stagingDirectory
}

function Invoke-Checks([string]$Features) {
    Invoke-Checked { cargo fmt --all --check } 'Falló cargo fmt.'
    Invoke-Checked {
        cargo clippy --workspace --all-targets --no-default-features `
            --features $Features --locked -- -D warnings
    } 'Falló cargo clippy.'
    Invoke-Checked {
        cargo test --workspace --no-default-features --features $Features --locked
    } 'Fallaron los tests.'
}

function Test-ManagedProfile([string]$Features) {
    Invoke-Checked {
        cargo test --package worklogger-desktop --no-default-features `
            --features $Features --locked `
            'defaults::tests::embedded_defaults_are_typed_and_non_zero' -- --exact
    } 'El perfil Managed no respeta el schema de configuración.'
}

function Invoke-Bundle([string]$Features, [string]$StagingDirectory, [string]$RustTarget) {
    Invoke-Checked {
        dx bundle --package worklogger-desktop --desktop --release --package-types nsis `
            --no-default-features --features $Features --target $RustTarget --locked `
            --out-dir $StagingDirectory
    } 'Falló el empaquetado NSIS.'
    if (-not (Test-Path $StagingDirectory)) {
        throw 'El empaquetado no produjo el directorio de salida esperado.'
    }
}

function Get-DioxusBuildDirectory(
    [string]$RepositoryRoot,
    [bool]$IncludeMcp
) {
    if ($IncludeMcp) { return $RepositoryRoot }
    $directory = Join-Path $RepositoryRoot 'desktop-app'
    $source = Join-Path $RepositoryRoot 'Dioxus.toml'
    $destination = Join-Path $directory 'Dioxus.toml'
    if (Test-Path $destination) {
        throw 'Existe desktop-app\Dioxus.toml; no se sobrescribirá una configuración local.'
    }
    Get-Content -Path $source | Where-Object { $_ -notmatch '^\s*external_bin\s*=' } |
        Set-Content -Path $destination -Encoding utf8
    return $directory
}

function Invoke-ConfiguredBundle(
    [string]$RepositoryRoot,
    [string]$Features,
    [string]$StagingDirectory,
    [string]$RustTarget,
    [bool]$IncludeMcp
) {
    $buildDirectory = Get-DioxusBuildDirectory $RepositoryRoot $IncludeMcp
    $appDirectory = Join-Path $RepositoryRoot $DioxusAppRelativePath
    if (Test-Path $appDirectory) {
        Remove-Item -Recurse -Force -Path $appDirectory
    }
    Push-Location $buildDirectory
    try {
        Invoke-Bundle $Features $StagingDirectory $RustTarget
    } finally {
        Pop-Location
        if (-not $IncludeMcp) {
            Remove-Item -Force -Path (Join-Path $buildDirectory 'Dioxus.toml')
        }
    }
}

function Copy-Installer([string]$StagingDirectory, [string]$Destination) {
    $installers = @(Get-ChildItem -Path $StagingDirectory -Filter '*.exe' -File -Recurse)
    if ($installers.Count -ne 1) {
        throw "Se esperaba un instalador y se encontraron $($installers.Count)."
    }
    Copy-Item -Force -Path $installers[0].FullName -Destination $Destination
}

function Get-DesktopExecutable([string]$Directory) {
    $executables = @(Get-ChildItem -Path $Directory -Filter '*.exe' -File | Where-Object {
        $_.Name -notlike 'worklogger-mcp*'
    })
    if ($executables.Count -ne 1) {
        throw "Se esperaba un ejecutable portable y se encontraron $($executables.Count)."
    }
    return $executables[0]
}

function New-PortableDirectory(
    [string]$RepositoryRoot,
    [string]$StagingDirectory,
    [string]$DistributionName,
    [string]$Version
) {
    $appDirectory = Join-Path $RepositoryRoot $DioxusAppRelativePath
    $sourceExecutable = Get-DesktopExecutable $appDirectory
    $portableDirectory = Join-Path $StagingDirectory "portable\Worklogger-$DistributionName-$Version"
    New-Item -ItemType Directory -Force -Path $portableDirectory | Out-Null
    Copy-Item -Path (Join-Path $appDirectory '*') -Destination $portableDirectory -Recurse -Force
    Move-Item -Path (Join-Path $portableDirectory $sourceExecutable.Name) `
        -Destination (Join-Path $portableDirectory $PortableExecutableName) -Force
    Copy-Item -Path (Join-Path $RepositoryRoot $PortableInstructionsRelativePath) `
        -Destination (Join-Path $portableDirectory 'LEEME.txt') -Force
    return $portableDirectory
}

function Assert-PortableDirectory([string]$PortableDirectory, [bool]$IncludeMcp) {
    $requiredPaths = @($PortableExecutableName, 'LEEME.txt')
    if ($IncludeMcp) { $requiredPaths += $McpExecutableName }
    foreach ($requiredPath in $requiredPaths) {
        if (-not (Test-Path (Join-Path $PortableDirectory $requiredPath) -PathType Leaf)) {
            throw "La distribución portable no contiene $requiredPath."
        }
    }
    foreach ($assetPattern in @('main-*.css', 'brand-logo-*.svg')) {
        $matches = @(Get-ChildItem -Path (Join-Path $PortableDirectory 'assets') -Filter $assetPattern -File)
        if ($matches.Count -eq 0) {
            throw "La distribución portable no contiene un asset $assetPattern."
        }
    }
}

function Copy-PortableMcpExecutable(
    [string]$RepositoryRoot,
    [string]$PortableDirectory,
    [string]$RustTarget,
    [bool]$IncludeMcp
) {
    if (-not $IncludeMcp) { return }
    $source = Join-Path $RepositoryRoot "target\$RustTarget\release\$McpExecutableName"
    if (-not (Test-Path $source -PathType Leaf)) {
        throw "No se encontró el ejecutable MCP compilado en $source."
    }
    Get-ChildItem -Path $PortableDirectory -Filter 'worklogger-mcp*.exe' -File |
        Remove-Item -Force
    Copy-Item -Force -Path $source -Destination (Join-Path $PortableDirectory $McpExecutableName)
}

function Write-Checksum([string]$Source, [string]$Destination, [string]$Label) {
    $fileHash = (Get-FileHash -Algorithm SHA256 $Source).Hash.ToLowerInvariant()
    Set-Content -Path $Destination -Encoding ascii -Value "$fileHash  $Label"
}

function Write-PortableArchive([string]$PortableDirectory, [string]$Destination) {
    Compress-Archive -Path $PortableDirectory -DestinationPath $Destination -CompressionLevel Optimal -Force
    Write-Checksum $Destination "${Destination}.sha256" (Split-Path -Leaf $Destination)
    Write-Host "Portable creada: $Destination"
}

Add-UserToolPaths
Import-VisualStudioEnvironment
Assert-BuildTools
$repositoryRoot = Get-RepositoryRoot
$managedProfile = if ($Edition -eq 'Managed') { Get-ManagedProfile $repositoryRoot } else { $null }
$distributionName = if ($Name) { $Name } elseif ($managedProfile) { $managedProfile.Company } else { 'Community' }
$features = Get-Features $Edition $McpAddons
$includeMcp = $McpAddons.Count -gt 0
$version = Get-Version $repositoryRoot
$safeName = Get-SafeName $distributionName
$output = if ($OutputDirectory) { $OutputDirectory } else { Join-Path $repositoryRoot 'dist' }
$staging = Get-StagingDirectory $repositoryRoot $safeName
$destination = Join-Path $output "Worklogger-$safeName-$version-Setup.exe"
$portableDestination = Join-Path $output "Worklogger-$safeName-$version-Portable.zip"

New-Item -ItemType Directory -Force -Path $output | Out-Null
if (Test-Path $staging) {
    Remove-Item -Recurse -Force -Path $staging
}

Push-Location $repositoryRoot
try {
    if ($managedProfile) {
        $env:WORKLOGGER_DISTRIBUTION_PROFILE = $managedProfile.Path
    } else {
        Remove-Item Env:WORKLOGGER_DISTRIBUTION_PROFILE -ErrorAction SilentlyContinue
    }
    if (-not $SkipChecks) {
        Invoke-Checks $features
    } elseif ($managedProfile) {
        Test-ManagedProfile $features
    }
    if ($includeMcp) {
        Invoke-McpBuild $repositoryRoot $WindowsRustTarget $McpAddons $Edition
    }
    Invoke-ConfiguredBundle $repositoryRoot $features $staging $WindowsRustTarget $includeMcp
    Copy-Installer $staging $destination
    Write-Checksum $destination "${destination}.sha256" (Split-Path -Leaf $destination)
    $portableDirectory = New-PortableDirectory $repositoryRoot $staging $safeName $version
    Copy-PortableMcpExecutable $repositoryRoot $portableDirectory $WindowsRustTarget $includeMcp
    Assert-PortableDirectory $portableDirectory $includeMcp
    Write-PortableArchive $portableDirectory $portableDestination
    if ($managedProfile) {
        $profileChecksum = Join-Path $output "Worklogger-$safeName-$version-Profile.sha256"
        Write-Checksum $managedProfile.Path $profileChecksum 'embedded-profile.json'
    }
    Write-Host "Instalador creado: $destination"
} finally {
    Remove-Item Env:WORKLOGGER_DISTRIBUTION_PROFILE -ErrorAction SilentlyContinue
    Pop-Location
}
