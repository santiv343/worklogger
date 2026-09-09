[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ManagedProfile,
    [string]$ManagedName,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$builder = Join-Path $PSScriptRoot 'build-windows.ps1'
$shared = @{}
if ($OutputDirectory) {
    $shared.OutputDirectory = $OutputDirectory
}

& $builder -Edition Community @shared
& $builder -Edition Managed -Profile $ManagedProfile -Name $ManagedName @shared
