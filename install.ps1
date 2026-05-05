param(
    [switch]$All,
    [string]$BinDir,
    [string]$Repo = "Ameyanagi/yomi",
    [string]$Version = "latest",
    [switch]$FromSource
)

$ErrorActionPreference = "Stop"

if (-not $BinDir -or $BinDir.Trim().Length -eq 0) {
    $BinDir = Join-Path $env:LOCALAPPDATA "Yomi\bin"
}

function Write-YomiInstallLog {
    param([string]$Message)
    Write-Host "yomi-install: $Message"
}

function Get-YomiTarget {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        throw "install.ps1 currently supports Windows user-space installs. Use ./install on macOS/Linux."
    }
    if ($arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
        throw "unsupported Windows architecture: $arch"
    }
    "x86_64-pc-windows-msvc"
}

function Install-YomiFromSource {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo is required for -FromSource"
    }
    Write-YomiInstallLog "building release binary with cargo"
    cargo build --release -p yomi-cli
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Copy-Item -Force "target\release\yomi.exe" (Join-Path $BinDir "yomi.exe")
}

function Install-YomiFromRelease {
    $target = Get-YomiTarget
    $asset = "yomi-$target.zip"
    if ($Version -eq "latest") {
        $url = "https://github.com/$Repo/releases/latest/download/$asset"
    } else {
        $url = "https://github.com/$Repo/releases/download/$Version/$asset"
    }

    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("yomi-install-" + [System.Guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $archive = Join-Path $tmp $asset
        Write-YomiInstallLog "downloading $asset"
        Invoke-WebRequest -Uri $url -OutFile $archive
        Expand-Archive -Force -Path $archive -DestinationPath $tmp
        $binary = Join-Path $tmp "yomi.exe"
        if (-not (Test-Path $binary)) {
            throw "archive did not contain yomi.exe"
        }
        New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
        Copy-Item -Force $binary (Join-Path $BinDir "yomi.exe")
    } finally {
        Remove-Item -Force -Recurse $tmp -ErrorAction SilentlyContinue
    }
}

function Add-YomiToUserPath {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($userPath) {
        $parts = $userPath -split ';' | Where-Object { $_ }
    }
    if ($parts -notcontains $BinDir) {
        $next = (@($parts) + $BinDir) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $next, "User")
        Write-YomiInstallLog "added $BinDir to the user PATH"
    }
    if (($env:Path -split ';') -notcontains $BinDir) {
        $env:Path = "$env:Path;$BinDir"
    }
}

function Install-YomiPowerShellIntegration {
    $profilePath = $PROFILE.CurrentUserAllHosts
    $profileDir = Split-Path -Parent $profilePath
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    if (-not (Test-Path $profilePath)) {
        New-Item -ItemType File -Force -Path $profilePath | Out-Null
    }

    $marker = "yomi shell integration"
    $content = Get-Content -Raw -Path $profilePath
    if ($content -like "*$marker*") {
        Write-YomiInstallLog "PowerShell integration already present in $profilePath"
        return
    }

    Add-Content -Path $profilePath -Value @"

# yomi shell integration
if (Get-Command yomi -ErrorAction SilentlyContinue) {
    yomi --powershell | Invoke-Expression
}
"@
    Write-YomiInstallLog "updated $profilePath"
}

if ($FromSource) {
    Install-YomiFromSource
} else {
    Install-YomiFromRelease
}

Write-YomiInstallLog "installed binary into $BinDir"
Add-YomiToUserPath

if ($All) {
    Install-YomiPowerShellIntegration
    Write-YomiInstallLog "restart PowerShell or reload your profile"
}
