<#
.SYNOPSIS
Undo one install session of install-ae-plugin.ps1.

.DESCRIPTION
Restores every file the session overwrote from its verified backup and removes
files the session added. Before changing anything it verifies all backups and
checks that every target still has either the installed or the original hash;
a file modified since the install stops the whole restore. Running it twice is
harmless. Sessions are undone one at a time; to go further back, restore older
sessions in reverse order (newest first).

.PARAMETER Session
Session folder name (or full path). Default: the newest session in -BackupRoot.

.PARAMETER DestinationRoot
Testing only: the fake root used for the install. Journal targets must lie in
its MediaCore folder.
#>
param(
    [string]$BackupRoot = '',
    [string]$Session = '',
    [string]$DestinationRoot = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ae-plugin-deploy.ps1')

Assert-AeClosed
if (-not $BackupRoot) { $BackupRoot = Get-DefaultBackupRoot }
$BackupRoot = Get-FullPath $BackupRoot
$roots = Get-AeRoots $DestinationRoot

if (-not $Session) {
    $latest = Get-ChildItem -LiteralPath $BackupRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'journal.json') } |
        Sort-Object Name | Select-Object -Last 1
    if (-not $latest) { throw "No backup session found in $BackupRoot" }
    $Session = $latest.FullName
} elseif (-not [IO.Path]::IsPathRooted($Session)) {
    $Session = Join-Path $BackupRoot $Session
}
$journalPath = Join-Path $Session 'journal.json'
if (-not (Test-Path -LiteralPath $journalPath)) { throw "No journal in $Session" }
$journal = Get-Content -LiteralPath $journalPath -Raw | ConvertFrom-Json
if ($journal.schema_version -ne 1 -or @($journal.files).Count -eq 0) { throw "Invalid journal: $journalPath" }
if ((Get-FullPath $journal.media_core) -ine (Get-FullPath $roots.MediaCore)) {
    throw "Journal belongs to a different MediaCore root: $($journal.media_core)"
}

# Verify everything before the first mutation.
foreach ($f in $journal.files) {
    if (-not (Test-PathUnder $f.target $roots.MediaCore)) { throw "Journal target outside MediaCore: $($f.target)" }
    Assert-NoReparsePoint $f.target
    if ($f.existed) {
        if (-not (Test-PathUnder $f.backup $Session)) { throw "Backup outside the session: $($f.backup)" }
        if ((Get-Sha256 $f.backup) -ne $f.original_sha256) { throw "Backup missing or corrupt: $($f.backup). Nothing was changed." }
    }
    $current = Get-Sha256 $f.target
    $ok = ($current -eq $f.installed_sha256) -or ($f.existed -and $current -eq $f.original_sha256) -or (-not $f.existed -and $null -eq $current)
    if (-not $ok) { throw "Target changed since install: $($f.target). Nothing was changed; resolve it manually." }
}

$table = @()
foreach ($f in $journal.files) {
    Assert-AeClosed
    $current = Get-Sha256 $f.target
    if ($f.existed) {
        if ($current -ne $f.original_sha256) { Copy-Item -LiteralPath $f.backup -Destination $f.target -Force }
        if ((Get-Sha256 $f.target) -ne $f.original_sha256) { throw "Restore hash mismatch: $($f.target)" }
        $status = 'Restored/verified'
    } else {
        if ($null -ne $current) { Remove-Item -LiteralPath $f.target -Force }
        if (Test-Path -LiteralPath $f.target) { throw "Could not remove $($f.target)" }
        $status = 'Removed (was added by install)'
    }
    $table += [pscustomobject]@{ Plugin = $f.plugin; Status = $status; Target = $f.target }
}
$table | Format-Table -AutoSize -Wrap
Write-Host "Restore of $Session complete. All hashes verified."
