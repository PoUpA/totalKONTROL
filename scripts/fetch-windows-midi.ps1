# Downloads the Windows.Devices.Midi2 NuGet package (Windows MIDI Services API)
# from Microsoft's GitHub releases and extracts it into src-tauri/vendor/windows-midi2.
# Needed only to build with the `windows-midi-services` feature.

$ErrorActionPreference = 'Stop'

$release = 'inbox-preview-11'
$package = 'Windows.Devices.Midi2.0.99.92-preview.11.nupkg'
$url = "https://github.com/microsoft/MIDI/releases/download/$release/$package"
# Its DLL ships in the winmidi installers: refuse any other file. The digest
# is the one GitHub lists for the release asset.
$sha256 = '13afc6e6da2adac309ab8c174352f46f1c0ebe2d79e65475bd6274defb73df74'

$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'src-tauri\vendor\windows-midi2'
$winmd = Join-Path $target 'ref\native\Windows.Devices.Midi2.winmd'

if (Test-Path $winmd) {
    Write-Host "Package already in $target"
    exit 0
}

$download = Join-Path ([System.IO.Path]::GetTempPath()) $package
Write-Host "Downloading $url (about 103 MB)..."
Invoke-WebRequest -Uri $url -OutFile $download -UseBasicParsing

$actual = (Get-FileHash -Algorithm SHA256 $download).Hash.ToLowerInvariant()
if ($actual -ne $sha256) {
    Remove-Item $download
    throw "Unexpected SHA-256 for ${package}: $actual (expected $sha256)"
}

# A .nupkg is a zip archive; Expand-Archive only accepts the .zip extension.
$zip = [System.IO.Path]::ChangeExtension($download, '.zip')
Move-Item -Force $download $zip
New-Item -ItemType Directory -Force $target | Out-Null
Expand-Archive -Path $zip -DestinationPath $target -Force
Remove-Item $zip

if (-not (Test-Path $winmd)) {
    throw "The package does not contain $winmd"
}
Write-Host "Package extracted into $target"
