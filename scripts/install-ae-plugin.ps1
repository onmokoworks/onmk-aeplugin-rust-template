<#
.SYNOPSIS
Build AE plug-ins into dist\ and optionally install them into Adobe MediaCore, safely.

.DESCRIPTION
Without -Install this only builds and copies <Name>.aex into dist\ (no admin
rights, nothing outside the repository is touched).

With -Install, for MediaCore\<Subfolder>\<Name>.aex:
  - refuses while After Effects (AfterFX) is running;
  - refuses if the artifact does not contain its match name (stale/wrong file);
  - refuses if any other .aex in MediaCore or in an AE "Support Files\Plug-ins"
    folder contains the same match name (ASCII or UTF-16);
  - backs up every file it will overwrite, verifies the backup hash and writes a
    journal before changing anything;
  - verifies the SHA-256 of every installed file;
  - is idempotent: files that already match are left alone and no backup
    session is created when nothing changes.
Undo with restore-ae-plugin.ps1. The real MediaCore folder is under Program
Files, so -Install needs an elevated PowerShell; build first without elevation
and then run the elevated install with -SkipBuild.

.PARAMETER DestinationRoot
Testing only: a fake root that contains MediaCore\ and optionally
"Adobe After Effects <ver>\Support Files\Plug-ins\". No admin rights needed.

.EXAMPLE
.\scripts\install-ae-plugin.ps1 -Plugin minimal
.\scripts\install-ae-plugin.ps1 -Plugin minimal,lab -Install -SkipBuild   # elevated
#>
param(
    # minimal, lab, probe. Several: -Plugin minimal,lab (also "minimal,lab" via -File).
    [string[]]$Plugin = @('minimal'),

    [string]$OutDir = '',

    [Alias('InstallMediaCore')]
    [switch]$Install,

    [switch]$SkipBuild,

    [string]$DestinationRoot = '',

    [string]$BackupRoot = '',

    [string]$Subfolder = 'onmk-aeplugin-rust-template'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ae-plugin-deploy.ps1')

$repoRoot = Get-FullPath (Join-Path $PSScriptRoot '..')
if (-not $OutDir) { $OutDir = Join-Path $repoRoot 'dist' }
$OutDir = Get-FullPath $OutDir
if ($Subfolder -match '[\\/:]|^\.\.?$' -or -not $Subfolder) { throw "Subfolder must be a single folder name: $Subfolder" }

$names = @($Plugin | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Select-Object -Unique)
if ($names.Count -eq 0) { throw 'No plug-in selected.' }
$entries = @()
foreach ($name in $names) {
    if (-not $AePluginCatalog.Contains($name)) { throw "Unknown plug-in '$name'. Known: $($AePluginCatalog.Keys -join ', ')" }
    $info = $AePluginCatalog[$name]
    $entries += [pscustomobject]@{
        Name = $name; Package = $info.Package; Dll = $info.Dll; Aex = $info.Aex
        MatchName = $info.MatchName; Source = (Join-Path $OutDir $info.Aex); Target = $null
    }
}

# ---- Build into dist\ ----
if (-not $SkipBuild) {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    Push-Location $repoRoot
    try {
        foreach ($e in $entries) {
            # Windows PowerShell 5.1 turns redirected native stderr (cargo's
            # progress output) into errors under 'Stop'; rely on the exit code.
            $previous = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try { cargo build -p $e.Package --release } finally { $ErrorActionPreference = $previous }
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $($e.Package)" }
            $dll = Join-Path $repoRoot "target\release\$($e.Dll)"
            if (-not (Test-Path -LiteralPath $dll)) { throw "Build output was not found: $dll" }
            Copy-Item -LiteralPath $dll -Destination $e.Source -Force
            Write-Host "Wrote $($e.Source)"
        }
    } finally {
        Pop-Location
    }
}

foreach ($e in $entries) {
    if (-not (Test-Path -LiteralPath $e.Source -PathType Leaf)) { throw "Artifact not found: $($e.Source)" }
    if (-not (Test-ContainsMatchName ([IO.File]::ReadAllBytes($e.Source)) $e.MatchName)) {
        throw "Artifact $($e.Source) does not contain match name '$($e.MatchName)'. Wrong or stale file?"
    }
    $e | Add-Member -NotePropertyName Sha256 -NotePropertyValue (Get-Sha256 $e.Source)
}

if (-not $Install) {
    $entries | Format-Table Name, Sha256, Source -AutoSize
    return
}

# ---- Install into MediaCore ----
Assert-AeClosed
if (-not $DestinationRoot -and -not (Test-IsAdministrator)) {
    throw 'Installing into the real MediaCore folder (under Program Files) needs an elevated PowerShell. Build first, then run elevated with -Install -SkipBuild.'
}
$roots = Get-AeRoots $DestinationRoot
$targetDir = Join-Path $roots.MediaCore $Subfolder
Assert-NoReparsePoint $targetDir
foreach ($e in $entries) { $e.Target = Join-Path $targetDir $e.Aex }

if (-not $BackupRoot) { $BackupRoot = Get-DefaultBackupRoot }
$BackupRoot = Get-FullPath $BackupRoot
Assert-BackupRootSeparate $BackupRoot (@($roots.Scan) + @($OutDir))

Assert-NoDuplicateMatchName $entries $roots.Scan

$pending = @($entries | Where-Object { (Get-Sha256 $_.Target) -ne $_.Sha256 })
if ($pending.Count -eq 0) {
    $entries | Format-Table Name, @{ n = 'Status'; e = { 'Already installed/verified' } }, Target -AutoSize -Wrap
    Write-Host 'Nothing to do; no backup session created.'
    return
}

# Journal and backups first, then mutate.
$session = Join-Path $BackupRoot ((Get-Date).ToUniversalTime().ToString('yyyyMMdd-HHmmss-fff'))
if (Test-Path -LiteralPath $session) { throw "Backup session already exists: $session" }
New-Item -ItemType Directory -Path (Join-Path $session 'files') -Force | Out-Null
$journal = [ordered]@{
    schema_version = 1
    created_utc    = (Get-Date).ToUniversalTime().ToString('o')
    media_core     = $roots.MediaCore
    files          = @()
}
foreach ($e in $pending) {
    if (Test-Path -LiteralPath $e.Target -PathType Container) { throw "Target is a directory: $($e.Target)" }
    $original = Get-Sha256 $e.Target
    $backup = $null
    if ($null -ne $original) {
        $backup = Join-Path (Join-Path $session 'files') $e.Aex
        Copy-Item -LiteralPath $e.Target -Destination $backup
        if ((Get-Sha256 $backup) -ne $original) { throw "Backup hash mismatch: $backup" }
    }
    $journal.files += [ordered]@{
        plugin = $e.Name; match_name = $e.MatchName; target = $e.Target
        existed = ($null -ne $original); original_sha256 = $original
        backup = $backup; installed_sha256 = $e.Sha256
    }
}
Save-JsonAtomic $journal (Join-Path $session 'journal.json')

New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
$table = @()
foreach ($e in $entries) {
    Assert-AeClosed
    $status = 'Already installed/verified'
    if ($pending -contains $e) {
        if ((Get-Sha256 $e.Source) -ne $e.Sha256) { throw "Artifact changed during install: $($e.Source)" }
        try {
            Copy-Item -LiteralPath $e.Source -Destination $e.Target -Force
        } catch {
            throw "Copy failed for $($e.Target): $($_.Exception.Message). Run restore-ae-plugin.ps1 -Session '$session' to undo."
        }
        $status = if (($journal.files | Where-Object { $_.target -eq $e.Target }).existed) { 'Replaced/verified' } else { 'Added/verified' }
    }
    if ((Get-Sha256 $e.Target) -ne $e.Sha256) { throw "Installed hash mismatch: $($e.Target). Run restore-ae-plugin.ps1 -Session '$session'." }
    $table += [pscustomobject]@{ Plugin = $e.Name; Status = $status; Target = $e.Target }
}
$table | Format-Table -AutoSize -Wrap
Write-Host "Install complete. All hashes verified. Backup session: $session"
