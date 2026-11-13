# Shared helpers for install-ae-plugin.ps1 / restore-ae-plugin.ps1.
# Dot-source only. Windows PowerShell 5.1 compatible.
Set-StrictMode -Version Latest

# Plug-ins this repository can build. MatchName must equal
# AE_Effect_Match_Name in the crate's build.rs.
$script:AePluginCatalog = [ordered]@{
    minimal = @{ Package = 'ae_minimal_plugin';       Dll = 'ae_minimal_plugin.dll';       Aex = 'AeMinimal.aex';  MatchName = 'ONMK AeMinimal' }
    lab     = @{ Package = 'ae_gpu_lab_plugin';       Dll = 'ae_gpu_lab_plugin.dll';       Aex = 'AeGpuLab.aex';   MatchName = 'ONMK AeGpuLab' }
    probe   = @{ Package = 'ae_gpu_lab_probe_plugin'; Dll = 'ae_gpu_lab_probe_plugin.dll'; Aex = 'AeGpuProbe.aex'; MatchName = 'ONMK AeGpuProbe' }
}

function Assert-AeClosed {
    if (Get-Process -Name AfterFX -ErrorAction SilentlyContinue) {
        throw 'After Effects (AfterFX) is running. Close it before installing or restoring plug-ins.'
    }
}

function Get-Sha256([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Test-IsAdministrator {
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-FullPath([string]$Path) {
    return [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
}

function Test-PathUnder([string]$Path, [string]$Root) {
    $p = Get-FullPath $Path
    $r = Get-FullPath $Root
    return ($p -ieq $r) -or $p.StartsWith($r + '\', [StringComparison]::OrdinalIgnoreCase)
}

# Refuse reparse points (junctions/symlinks) anywhere on the path so a copy can
# never be redirected outside the intended root.
function Assert-NoReparsePoint([string]$Path) {
    $check = Get-FullPath $Path
    while ($check) {
        if (Test-Path -LiteralPath $check) {
            $item = Get-Item -LiteralPath $check -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point not allowed: $check" }
        }
        $check = [IO.Path]::GetDirectoryName($check)
    }
}

# MediaCore is shared by AE and Premiere. The per-version AE Plug-ins folders
# are scanned for duplicates only; this script never writes there.
function Get-AeRoots([string]$DestinationRoot) {
    if ($DestinationRoot) {
        $base = Get-FullPath $DestinationRoot
        $mediaCore = Join-Path $base 'MediaCore'
        $programFiles = $base
    } else {
        $common = [Environment]::GetFolderPath('CommonProgramFiles')
        if (-not $common) { throw 'CommonProgramFiles could not be resolved.' }
        $mediaCore = Join-Path $common 'Adobe\Common\Plug-ins\7.0\MediaCore'
        $programFiles = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Adobe'
    }
    $scan = @($mediaCore)
    if (Test-Path -LiteralPath $programFiles) {
        foreach ($ae in Get-ChildItem -LiteralPath $programFiles -Directory -Filter 'Adobe After Effects *' -ErrorAction SilentlyContinue) {
            $plugins = Join-Path $ae.FullName 'Support Files\Plug-ins'
            if (Test-Path -LiteralPath $plugins) { $scan += $plugins }
        }
    }
    return [pscustomobject]@{ MediaCore = $mediaCore; Scan = $scan }
}

function Initialize-ByteSearch {
    if (-not ('AePluginByteSearch' -as [type])) {
        Add-Type -TypeDefinition @'
public static class AePluginByteSearch {
    public static bool Contains(byte[] data, byte[] needle) {
        if (needle.Length == 0) return false;
        for (int i = 0; i <= data.Length - needle.Length; i++) {
            if (data[i] != needle[0]) continue;
            int j = 1;
            while (j < needle.Length && data[i + j] == needle[j]) j++;
            if (j == needle.Length) return true;
        }
        return false;
    }
}
'@
    }
}

function Test-ContainsMatchName([byte[]]$Bytes, [string]$MatchName) {
    Initialize-ByteSearch
    foreach ($encoding in @([Text.Encoding]::ASCII, [Text.Encoding]::Unicode, [Text.Encoding]::BigEndianUnicode)) {
        if ([AePluginByteSearch]::Contains($Bytes, $encoding.GetBytes($MatchName))) { return $true }
    }
    return $false
}

# AE refuses to load (or loads an arbitrary one of) two plug-ins with the same
# match name. Look for any other .aex that contains one of our match names in
# ASCII or UTF-16, in MediaCore and every AE Plug-ins folder.
function Assert-NoDuplicateMatchName($Entries, [string[]]$ScanRoots) {
    $own = @{}
    foreach ($e in $Entries) { $own[(Get-FullPath $e.Target)] = $true }
    foreach ($root in $ScanRoots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $queue = New-Object 'System.Collections.Generic.Queue[string]'
        $queue.Enqueue($root)
        while ($queue.Count -gt 0) {
            $dir = $queue.Dequeue()
            foreach ($item in Get-ChildItem -LiteralPath $dir -Force) {
                if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                    Write-Warning "Not scanned for duplicates (reparse point): $($item.FullName)"
                    continue
                }
                if ($item.PSIsContainer) { $queue.Enqueue($item.FullName); continue }
                if ($item.Extension -ine '.aex' -or $own.ContainsKey((Get-FullPath $item.FullName))) { continue }
                $bytes = [IO.File]::ReadAllBytes($item.FullName)
                foreach ($e in $Entries) {
                    if (Test-ContainsMatchName $bytes $e.MatchName) {
                        throw "Possible duplicate of '$($e.MatchName)': $($item.FullName). Nothing was changed. Remove or restore that file first."
                    }
                }
            }
        }
    }
}

function Save-JsonAtomic($Object, [string]$Path) {
    $tmp = $Path + '.pending'
    $Object | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $tmp -Encoding UTF8
    Move-Item -LiteralPath $tmp -Destination $Path -Force
}

function Get-DefaultBackupRoot {
    return Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'onmk-aeplugin-rust-template\install-backups'
}

function Assert-BackupRootSeparate([string]$BackupRoot, [string[]]$Roots) {
    foreach ($root in $Roots) {
        if ((Test-PathUnder $BackupRoot $root) -or (Test-PathUnder $root $BackupRoot)) {
            throw "Backup root must be outside the plug-in folders: $BackupRoot"
        }
    }
}
