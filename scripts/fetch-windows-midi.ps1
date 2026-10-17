# Downloads the Windows.Devices.Midi2 NuGet package (Windows MIDI Services API)
# from Microsoft's GitHub releases and extracts it into src-tauri/vendor/windows-midi2.
# Needed only to build with the `windows-midi-services` feature.

$ErrorActionPreference = 'Stop'

$release = 'inbox-preview-11'
$package = 'Windows.Devices.Midi2.0.99.92-preview.11.nupkg'
$url = "https://github.com/microsoft/MIDI/releases/download/$release/$package"

$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'src-tauri\vendor\windows-midi2'
$winmd = Join-Path $target 'ref\native\Windows.Devices.Midi2.winmd'

if (Test-Path $winmd) {
    Write-Host "Paquet déjà présent dans $target"
    exit 0
}

$download = Join-Path ([System.IO.Path]::GetTempPath()) $package
Write-Host "Téléchargement de $url (environ 103 Mo)..."
Invoke-WebRequest -Uri $url -OutFile $download -UseBasicParsing

# A .nupkg is a zip archive; Expand-Archive only accepts the .zip extension.
$zip = [System.IO.Path]::ChangeExtension($download, '.zip')
Move-Item -Force $download $zip
New-Item -ItemType Directory -Force $target | Out-Null
Expand-Archive -Path $zip -DestinationPath $target -Force
Remove-Item $zip

if (-not (Test-Path $winmd)) {
    throw "Le paquet ne contient pas $winmd"
}
Write-Host "Paquet extrait dans $target"
