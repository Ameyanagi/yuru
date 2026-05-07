param(
    [switch]$All,
    [string]$BinDir,
    [string]$Repo = "Ameyanagi/yuru",
    [string]$Version = "latest",
    [ValidateSet("ask", "plain", "ja", "ko", "zh", "auto", "none")]
    [string]$DefaultLang = $(if ($env:YURU_INSTALL_DEFAULT_LANG) { $env:YURU_INSTALL_DEFAULT_LANG } else { "ask" }),
    [string]$PreviewCommand = $(if ($env:YURU_INSTALL_PREVIEW_COMMAND) { $env:YURU_INSTALL_PREVIEW_COMMAND } else { "ask" }),
    [string]$PreviewTextExtensions = $(if ($env:YURU_INSTALL_PREVIEW_TEXT_EXTENSIONS) { $env:YURU_INSTALL_PREVIEW_TEXT_EXTENSIONS } else { "txt,md,markdown,rst,toml,json,jsonl,yaml,yml,csv,tsv,log,rs,py,js,jsx,ts,tsx,go,java,c,h,cpp,hpp,cs,rb,php,sh,bash,zsh,fish,ps1,sql,html,htm,css,scss,xml" }),
    [ValidateSet("ask", "none", "halfblocks", "sixel", "kitty", "iterm2")]
    [string]$PreviewImageProtocol = $(if ($env:YURU_INSTALL_PREVIEW_IMAGE_PROTOCOL) { $env:YURU_INSTALL_PREVIEW_IMAGE_PROTOCOL } else { "ask" }),
    [ValidateSet("ask", "auto", "fd", "fdfind", "find")]
    [string]$PathBackend = $(if ($env:YURU_INSTALL_PATH_BACKEND) { $env:YURU_INSTALL_PATH_BACKEND } else { "ask" }),
    [string]$Bindings = $(if ($env:YURU_INSTALL_BINDINGS) { $env:YURU_INSTALL_BINDINGS } else { "ask" }),
    [switch]$NoConfig,
    [switch]$FromSource
)

$ErrorActionPreference = "Stop"
$script:YuruSelectedPathBackend = ""

if (-not $BinDir -or $BinDir.Trim().Length -eq 0) {
    $BinDir = Join-Path $env:LOCALAPPDATA "Yuru\bin"
}

function Write-YuruInstallLog {
    param([string]$Message)
    Write-Host "yuru-install: $Message"
}

function Get-YuruConfigPath {
    if ($env:YURU_CONFIG_FILE) { return $env:YURU_CONFIG_FILE }
    if ($env:APPDATA) { return (Join-Path $env:APPDATA "yuru\config.toml") }
    return (Join-Path $HOME ".config\yuru\config.toml")
}

function Test-YuruCanPrompt {
    if (-not [Environment]::UserInteractive) { return $false }
    try {
        return -not [Console]::IsInputRedirected
    } catch {
        return $true
    }
}

function Read-YuruDefaultLanguage {
    if ($DefaultLang -ne "ask") { return $DefaultLang }
    if (-not (Test-YuruCanPrompt)) { return "none" }

    while ($true) {
        $answer = Read-Host "Choose Yuru default language [plain/ja/ko/zh/auto/none] (none)"
        if ([string]::IsNullOrWhiteSpace($answer)) { return "none" }
        switch ($answer.Trim()) {
            "plain" { return "plain" }
            "ja" { return "ja" }
            "ko" { return "ko" }
            "zh" { return "zh" }
            "auto" { return "auto" }
            "none" { return "none" }
            default { Write-Host "Please enter plain, ja, ko, zh, auto, or none." }
        }
    }
}

function Read-YuruPreviewImageProtocol {
    if ($PreviewImageProtocol -ne "ask") { return $PreviewImageProtocol }
    if (-not (Test-YuruCanPrompt)) { return "none" }

    while ($true) {
        $answer = Read-Host "Choose Yuru preview image protocol [none/halfblocks/sixel/kitty/iterm2] (none)"
        if ([string]::IsNullOrWhiteSpace($answer)) { return "none" }
        switch ($answer.Trim().ToLowerInvariant()) {
            "none" { return "none" }
            "halfblocks" { return "halfblocks" }
            "sixel" { return "sixel" }
            "kitty" { return "kitty" }
            "iterm2" { return "iterm2" }
            default { Write-Host "Please enter none, halfblocks, sixel, kitty, or iterm2." }
        }
    }
}

function Read-YuruPreviewCommand {
    if ($PreviewCommand -ne "ask") { return $PreviewCommand }
    if (-not (Test-YuruCanPrompt)) { return "auto" }

    while ($true) {
        $answer = Read-Host "Choose Yuru preview command [auto/none/custom] (auto)"
        if ([string]::IsNullOrWhiteSpace($answer)) { return "auto" }
        switch ($answer.Trim().ToLowerInvariant()) {
            "auto" { return "auto" }
            "none" { return "none" }
            "custom" {
                $custom = Read-Host "Preview shell command"
                if (-not [string]::IsNullOrWhiteSpace($custom)) { return $custom }
                Write-Host "Please enter a shell command, auto, or none."
            }
            default { return $answer.Trim() }
        }
    }
}

function Read-YuruPreviewTextExtensions {
    if (-not (Test-YuruCanPrompt)) { return $PreviewTextExtensions }
    $answer = Read-Host "Preview text extensions ($PreviewTextExtensions)"
    if ([string]::IsNullOrWhiteSpace($answer)) { return $PreviewTextExtensions }
    return $answer.Trim()
}

function Read-YuruPathBackend {
    if ($PathBackend -ne "ask") { return $PathBackend }
    if (-not (Test-YuruCanPrompt)) { return "auto" }

    while ($true) {
        $answer = Read-Host "Choose Yuru shell path backend [auto/fd/fdfind/find] (auto)"
        if ([string]::IsNullOrWhiteSpace($answer)) { return "auto" }
        switch ($answer.Trim().ToLowerInvariant()) {
            "auto" { return "auto" }
            "fd" { return "fd" }
            "fdfind" { return "fdfind" }
            "find" { return "find" }
            default { Write-Host "Please enter auto, fd, fdfind, or find." }
        }
    }
}

function Test-YuruBindingList {
    param([string]$Value)
    if ($Value -in @("ask", "all", "none")) { return $true }
    foreach ($item in ($Value -split ",")) {
        $trimmed = $item.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed)) { continue }
        if ($trimmed -notin @("ctrl-t", "ctrl-r", "alt-c", "completion")) {
            return $false
        }
    }
    return $true
}

function Read-YuruYesNo {
    param(
        [string]$Prompt,
        [bool]$DefaultYes = $true
    )
    $suffix = $(if ($DefaultYes) { "Y/n" } else { "y/N" })
    while ($true) {
        $answer = Read-Host "$Prompt [$suffix]"
        if ([string]::IsNullOrWhiteSpace($answer)) { return $DefaultYes }
        switch ($answer.Trim().ToLowerInvariant()) {
            "y" { return $true }
            "yes" { return $true }
            "n" { return $false }
            "no" { return $false }
            default { Write-Host "Please enter yes or no." }
        }
    }
}

function Read-YuruShellBindings {
    if (-not (Test-YuruBindingList $Bindings)) {
        throw "unsupported shell bindings '$Bindings'; expected ask, all, none, or a comma-separated list of ctrl-t, ctrl-r, alt-c, completion"
    }
    if ($Bindings -ne "ask") { return $Bindings }
    if (-not (Test-YuruCanPrompt)) { return "all" }

    while ($true) {
        $answer = Read-Host "Choose Yuru shell bindings [all/custom/none] (all)"
        if ([string]::IsNullOrWhiteSpace($answer)) { return "all" }
        switch ($answer.Trim().ToLowerInvariant()) {
            "all" { return "all" }
            "none" { return "none" }
            "custom" {
                $selected = New-Object System.Collections.Generic.List[string]
                if (Read-YuruYesNo "Enable CTRL-T file search?" $true) { $selected.Add("ctrl-t") }
                if (Read-YuruYesNo "Enable CTRL-R history search?" $true) { $selected.Add("ctrl-r") }
                if (Read-YuruYesNo "Enable ALT-C directory jump?" $true) { $selected.Add("alt-c") }
                if (Read-YuruYesNo "Enable **<TAB> path completion?" $true) { $selected.Add("completion") }
                if ($selected.Count -eq 0) { return "none" }
                return ($selected -join ",")
            }
            default {
                if (Test-YuruBindingList $answer) { return $answer }
                Write-Host "Please enter all, custom, none, or a comma-separated binding list."
            }
        }
    }
}

function ConvertTo-YuruTomlString {
    param([string]$Value)
    return $Value.Replace('\', '\\').Replace('"', '\"')
}

function ConvertTo-YuruTomlStringArray {
    param([string]$Value)
    $items = @()
    foreach ($item in ($Value -split ",")) {
        $trimmed = $item.Trim().TrimStart(".")
        if (-not [string]::IsNullOrWhiteSpace($trimmed)) {
            $items += '"' + (ConvertTo-YuruTomlString $trimmed) + '"'
        }
    }
    return ($items -join ", ")
}

function ConvertTo-YuruPowerShellSingleQuoted {
    param([string]$Value)
    return "'" + $Value.Replace("'", "''") + "'"
}

function Get-YuruPreviewOptions {
    param([string]$Command)
    switch ($Command.Trim().ToLowerInvariant()) {
        "auto" { return "--preview-auto" }
        "none" { return "" }
        default { return "--preview " + (ConvertTo-YuruPowerShellSingleQuoted $Command) }
    }
}

function Install-YuruConfig {
    param([string]$ConfigBindings = "")
    if ($NoConfig) {
        Write-YuruInstallLog "skipping user config"
        return
    }

    $selectedLang = Read-YuruDefaultLanguage
    $selectedPreviewCommand = Read-YuruPreviewCommand
    $selectedPreviewCommandToml = ConvertTo-YuruTomlString $selectedPreviewCommand
    $selectedPreviewTextExtensions = Read-YuruPreviewTextExtensions
    $selectedPreviewTextExtensionsToml = ConvertTo-YuruTomlStringArray $selectedPreviewTextExtensions
    $selectedPreviewImageProtocol = Read-YuruPreviewImageProtocol
    $selectedPathBackend = ""
    if ($ConfigBindings) {
        $selectedPathBackend = Read-YuruPathBackend
    }
    $script:YuruSelectedPathBackend = $selectedPathBackend
    $ctrlTCommand = "Get-YuruPathItems ."
    $ctrlTOpts = Get-YuruPreviewOptions $selectedPreviewCommand
    $altCCommand = "Get-YuruDirItems ."
    $altCOpts = Get-YuruPreviewOptions $selectedPreviewCommand
    $configPath = Get-YuruConfigPath
    $configDir = Split-Path -Parent $configPath
    if (-not [string]::IsNullOrWhiteSpace($configDir)) {
        New-Item -ItemType Directory -Force -Path $configDir | Out-Null
    }

    $lines = @()
    if (Test-Path -LiteralPath $configPath) {
        $sourceLines = @(Get-Content -LiteralPath $configPath)
        $inDefaults = $false
        $sawDefaults = $false
        $wroteLang = $false
        $inPreview = $false
        $sawPreview = $false
        $wrotePreview = $false
        $inShell = $false
        $sawShell = $false
        $wroteShell = $false
        foreach ($line in $sourceLines) {
            if ($line -match '^\s*\[defaults\]\s*$') {
                $lines += $line
                $inDefaults = $true
                $inPreview = $false
                $inShell = $false
                $sawDefaults = $true
                if ($selectedLang -ne "none") {
                    $lines += "lang = `"$selectedLang`""
                    $wroteLang = $true
                }
                continue
            }
            if ($line -match '^\s*\[preview\]\s*$') {
                $lines += $line
                $inDefaults = $false
                $inPreview = $true
                $inShell = $false
                $sawPreview = $true
                $lines += "command = `"$selectedPreviewCommandToml`""
                $lines += "text_extensions = [$selectedPreviewTextExtensionsToml]"
                $lines += "image_protocol = `"$selectedPreviewImageProtocol`""
                $wrotePreview = $true
                continue
            }
            if ($line -match '^\s*\[shell\]\s*$') {
                $lines += $line
                $inDefaults = $false
                $inPreview = $false
                $inShell = $true
                $sawShell = $true
                if ($ConfigBindings) {
                    $lines += "bindings = `"$ConfigBindings`""
                    $lines += "path_backend = `"$selectedPathBackend`""
                    $lines += "ctrl_t_command = `"$ctrlTCommand`""
                    $lines += "ctrl_t_opts = `"$ctrlTOpts`""
                    $lines += "alt_c_command = `"$altCCommand`""
                    $lines += "alt_c_opts = `"$altCOpts`""
                    $wroteShell = $true
                }
                continue
            }
            if ($line -match '^\s*\[') {
                $inDefaults = $false
                $inPreview = $false
                $inShell = $false
                $lines += $line
                continue
            }
            if ($inDefaults -and $line -match '^\s*lang\s*=') {
                continue
            }
            if ($inPreview -and $line -match '^\s*command\s*=') {
                continue
            }
            if ($inPreview -and $line -match '^\s*text_extensions\s*=') {
                continue
            }
            if ($inPreview -and $line -match '^\s*image_protocol\s*=') {
                continue
            }
            if ($ConfigBindings -and $inShell -and $line -match '^\s*(bindings|ctrl_t_command|ctrl_t_opts|alt_c_command|alt_c_opts)\s*=') {
                continue
            }
            if ($ConfigBindings -and $inShell -and $line -match '^\s*path_backend\s*=') {
                continue
            }
            $lines += $line
        }
        if (-not $sawDefaults) {
            $lines += ""
            $lines += "[defaults]"
            if ($selectedLang -ne "none") {
                $lines += "lang = `"$selectedLang`""
                $wroteLang = $true
            }
            $lines += "load_fzf_defaults = `"safe`""
            $lines += "fzf_compat = `"warn`""
        }
        if (-not $sawPreview) {
            $lines += ""
            $lines += "[preview]"
            $lines += "command = `"$selectedPreviewCommandToml`""
            $lines += "text_extensions = [$selectedPreviewTextExtensionsToml]"
            $lines += "image_protocol = `"$selectedPreviewImageProtocol`""
        }
        if ($ConfigBindings -and -not $sawShell) {
            $lines += ""
            $lines += "[shell]"
            $lines += "bindings = `"$ConfigBindings`""
            $lines += "path_backend = `"$selectedPathBackend`""
            $lines += "ctrl_t_command = `"$ctrlTCommand`""
            $lines += "ctrl_t_opts = `"$ctrlTOpts`""
            $lines += "alt_c_command = `"$altCCommand`""
            $lines += "alt_c_opts = `"$altCOpts`""
        }
    } else {
        $lines += "[defaults]"
        if ($selectedLang -ne "none") {
            $lines += "lang = `"$selectedLang`""
        }
        $lines += "load_fzf_defaults = `"safe`""
        $lines += "fzf_compat = `"warn`""
        $lines += ""
        $lines += "[preview]"
        $lines += "command = `"$selectedPreviewCommandToml`""
        $lines += "text_extensions = [$selectedPreviewTextExtensionsToml]"
        $lines += "image_protocol = `"$selectedPreviewImageProtocol`""
        if ($ConfigBindings) {
            $lines += ""
            $lines += "[shell]"
            $lines += "bindings = `"$ConfigBindings`""
            $lines += "path_backend = `"$selectedPathBackend`""
            $lines += "ctrl_t_command = `"$ctrlTCommand`""
            $lines += "ctrl_t_opts = `"$ctrlTOpts`""
            $lines += "alt_c_command = `"$altCCommand`""
            $lines += "alt_c_opts = `"$altCOpts`""
        }
    }
    Set-Content -LiteralPath $configPath -Value $lines

    if ($selectedLang -eq "none") {
        Write-YuruInstallLog "left default language unset in $configPath"
    } else {
        Write-YuruInstallLog "configured default language '$selectedLang' in $configPath"
    }
    if ($ConfigBindings) {
        Write-YuruInstallLog "configured shell bindings '$ConfigBindings' in $configPath"
        Write-YuruInstallLog "configured shell path backend '$selectedPathBackend' in $configPath"
    }
    Write-YuruInstallLog "configured preview command '$selectedPreviewCommand' in $configPath"
    Write-YuruInstallLog "configured preview image protocol '$selectedPreviewImageProtocol' in $configPath"
}

function Get-YuruTarget {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        throw "install.ps1 currently supports Windows user-space installs. Use ./install on macOS/Linux."
    }
    if ($arch -ne [System.Runtime.InteropServices.Architecture]::X64) {
        throw "unsupported Windows architecture: $arch"
    }
    "x86_64-pc-windows-msvc"
}

function Install-YuruFromSource {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "cargo is required for -FromSource"
    }
    Write-YuruInstallLog "building release binary with cargo"
    cargo build --release -p yuru
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Copy-Item -Force "target\release\yuru.exe" (Join-Path $BinDir "yuru.exe")
}

function Test-YuruAssetChecksum {
    param(
        [string]$AssetPath,
        [string]$AssetName,
        [string]$BaseUrl,
        [string]$TempDir
    )

    $sumsPath = Join-Path $TempDir "SHA256SUMS"
    Write-YuruInstallLog "downloading SHA256SUMS"
    Invoke-WebRequest -Uri "$BaseUrl/SHA256SUMS" -OutFile $sumsPath

    $line = Get-Content -Path $sumsPath | Where-Object {
        $parts = $_ -split '\s+'
        $parts.Count -ge 2 -and ($parts[1] -eq $AssetName -or $parts[1] -eq "*$AssetName")
    } | Select-Object -First 1
    if (-not $line) {
        throw "SHA256SUMS did not contain $AssetName"
    }

    $expected = (($line -split '\s+')[0]).ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $AssetPath).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "checksum mismatch for ${AssetName}: expected $expected, got $actual"
    }
    Write-YuruInstallLog "verified $AssetName checksum"
}

function Install-YuruFromRelease {
    $target = Get-YuruTarget
    $asset = "yuru-$target.zip"
    if ($Version -eq "latest") {
        $baseUrl = "https://github.com/$Repo/releases/latest/download"
    } else {
        $baseUrl = "https://github.com/$Repo/releases/download/$Version"
    }
    $url = "$baseUrl/$asset"

    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("yuru-install-" + [System.Guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $archive = Join-Path $tmp $asset
        Write-YuruInstallLog "downloading $asset"
        Invoke-WebRequest -Uri $url -OutFile $archive
        Test-YuruAssetChecksum -AssetPath $archive -AssetName $asset -BaseUrl $baseUrl -TempDir $tmp
        Expand-Archive -Force -Path $archive -DestinationPath $tmp
        $binary = Join-Path $tmp "yuru.exe"
        if (-not (Test-Path $binary)) {
            throw "archive did not contain yuru.exe"
        }
        New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
        Copy-Item -Force $binary (Join-Path $BinDir "yuru.exe")
    } finally {
        Remove-Item -Force -Recurse $tmp -ErrorAction SilentlyContinue
    }
}

function Add-YuruToUserPath {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($userPath) {
        $parts = $userPath -split ';' | Where-Object { $_ }
    }
    if ($parts -notcontains $BinDir) {
        $next = (@($parts) + $BinDir) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $next, "User")
        Write-YuruInstallLog "added $BinDir to the user PATH"
    }
    if (($env:Path -split ';') -notcontains $BinDir) {
        $env:Path = "$env:Path;$BinDir"
    }
}

function Install-YuruPowerShellIntegration {
    $profilePath = $PROFILE.CurrentUserAllHosts
    $profileDir = Split-Path -Parent $profilePath
    New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    if (-not (Test-Path $profilePath)) {
        New-Item -ItemType File -Force -Path $profilePath | Out-Null
    }

    $marker = "yuru shell integration"
    $content = Get-Content -Raw -Path $profilePath
    if ($content -like "*$marker*") {
        Write-YuruInstallLog "PowerShell integration already present in $profilePath"
        return
    }

    $installedBinary = (Join-Path $BinDir "yuru.exe").Replace("'", "''")
    Add-Content -Path $profilePath -Value @"

# yuru shell integration
`$env:YURU_BIN = '$installedBinary'
if (Test-Path -LiteralPath `$env:YURU_BIN) {
    & `$env:YURU_BIN --powershell | Invoke-Expression
}
"@
    Write-YuruInstallLog "updated $profilePath"
}

function Test-YuruBindingsUseFd {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return $false }
    $items = @($Value -split '[,\s]+' | Where-Object { $_ })
    foreach ($item in $items) {
        if ($item -in @("all", "ctrl-t", "alt-c", "completion", "tab", "path-completion")) {
            return $true
        }
    }
    return $false
}

function Show-YuruFdSuggestion {
    param(
        [string]$ConfigBindings,
        [string]$PathBackend = "auto"
    )
    if (-not (Test-YuruBindingsUseFd $ConfigBindings)) { return }

    if ($PathBackend -eq "find") {
        Write-YuruInstallLog "using Get-ChildItem for shell path search"
    } elseif ($PathBackend -ne "fdfind" -and (Get-Command fd -ErrorAction SilentlyContinue)) {
        Write-YuruInstallLog "found fd for faster shell path search"
    } elseif ($PathBackend -ne "fd" -and (Get-Command fdfind -ErrorAction SilentlyContinue)) {
        Write-YuruInstallLog "found fdfind for faster shell path search"
    } elseif ($PathBackend -eq "fd") {
        Write-YuruInstallLog "suggestion: install fd for the selected shell path backend; Yuru falls back to Get-ChildItem"
    } elseif ($PathBackend -eq "fdfind") {
        Write-YuruInstallLog "suggestion: install fdfind for the selected shell path backend; Yuru falls back to Get-ChildItem"
    } else {
        Write-YuruInstallLog "suggestion: install fd for faster CTRL-T, ALT-C, and completion; Yuru falls back to Get-ChildItem"
    }
}

if ($FromSource) {
    Install-YuruFromSource
} else {
    Install-YuruFromRelease
}

Write-YuruInstallLog "installed binary into $BinDir"
Add-YuruToUserPath
$configBindings = ""
if ($All -or $Bindings -ne "ask") {
    $configBindings = Read-YuruShellBindings
}
Install-YuruConfig -ConfigBindings $configBindings
Show-YuruFdSuggestion -ConfigBindings $configBindings -PathBackend $script:YuruSelectedPathBackend

if ($All) {
    Install-YuruPowerShellIntegration
    Write-YuruInstallLog "restart PowerShell or reload your profile"
}
