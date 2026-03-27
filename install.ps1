#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$Repo = "4fuu/open-browser-cli"
$BinName = "browser-cli"
$InstallDir = "$env:LOCALAPPDATA\Programs\browser-cli"

# resolve version
if (-not $Version) {
    $release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
    $Version = $release.tag_name
}

$Target = "x86_64-pc-windows-msvc"
$DownloadUrl = "https://github.com/$Repo/releases/download/$Version/${BinName}-${Target}.zip"

Write-Host "Installing $BinName $Version -> $InstallDir"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$TmpZip = [System.IO.Path]::GetTempFileName() + ".zip"
try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TmpZip -UseBasicParsing
    Expand-Archive -Path $TmpZip -DestinationPath $InstallDir -Force
} finally {
    Remove-Item -Force -ErrorAction SilentlyContinue $TmpZip
}

Write-Host "Installed: $InstallDir\$BinName.exe"

# add to user PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to user PATH (restart terminal to take effect)"
} else {
    Write-Host "$InstallDir is already in PATH"
}
