param(
    [ValidateSet("lab", "probe")]
    [string]$Plugin = "lab",

    [string]$OutDir = "",

    [switch]$InstallMediaCore
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

$pluginMap = @{
    lab = @{
        Package = "ae_gpu_lab_plugin"
        Dll = "ae_gpu_lab_plugin.dll"
        Aex = "AeGpuLab.aex"
    }
    probe = @{
        Package = "ae_gpu_lab_probe_plugin"
        Dll = "ae_gpu_lab_probe_plugin.dll"
        Aex = "AeGpuProbe.aex"
    }
}

$entry = $pluginMap[$Plugin]

Push-Location $repoRoot
try {
    cargo build -p $entry.Package --release

    $dllPath = Join-Path $repoRoot "target\release\$($entry.Dll)"
    if (-not (Test-Path -LiteralPath $dllPath)) {
        throw "Build output was not found: $dllPath"
    }

    if ([string]::IsNullOrWhiteSpace($OutDir)) {
        $OutDir = Join-Path $repoRoot "dist"
    }
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

    $aexPath = Join-Path $OutDir $entry.Aex
    Copy-Item -LiteralPath $dllPath -Destination $aexPath -Force
    Write-Host "Wrote $aexPath"

    if ($InstallMediaCore) {
        $commonProgramFiles = [Environment]::GetFolderPath("CommonProgramFiles")
        if ([string]::IsNullOrWhiteSpace($commonProgramFiles)) {
            throw "CommonProgramFiles folder could not be resolved."
        }

        $mediaCore = Join-Path $commonProgramFiles "Adobe\Common\Plug-ins\7.0\MediaCore"
        $dest = Join-Path $mediaCore $entry.Aex
        $copyCommand = "New-Item -ItemType Directory -Force -Path '$mediaCore' | Out-Null; Copy-Item -LiteralPath '$aexPath' -Destination '$dest' -Force"

        Start-Process PowerShell `
            -Verb RunAs `
            -Wait `
            -WindowStyle Hidden `
            -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", $copyCommand)

        Write-Host "Installed $dest"
    }
}
finally {
    Pop-Location
}
