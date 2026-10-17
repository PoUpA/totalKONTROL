# Unblocks the Windows MIDI service after bug microsoft/MIDI#1047 (fixed in the
# late-November 2026 Windows update). Run in an administrator PowerShell, after
# closing totalKONTROL, the DAW and any other MIDI app.

$before = (Get-CimInstance Win32_Service -Filter "Name='midisrv'").ProcessId
Write-Host "PID avant : $before"
taskkill /F /IM midisrv.exe
Start-Sleep -Seconds 1
Start-Service midisrv
$after = (Get-CimInstance Win32_Service -Filter "Name='midisrv'").ProcessId
Write-Host "PID après : $after"
if ($after -and $after -ne $before) {
    Write-Host "Service redémarré."
} else {
    Write-Host "ÉCHEC : le service n'a pas redémarré."
}
