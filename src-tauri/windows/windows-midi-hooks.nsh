; NSIS hooks for the installer built with the windows-midi-services feature
; (tauri.windows-midi.conf.json).
;
; The bundled Windows.Devices.Midi2.dll comes with a "go live" license that
; forbids installing it on Windows versions older than Windows 11 25H2
; (build 26200), so the installer refuses to run there.

!include LogicLib.nsh
!include WinVer.nsh

!define TOTALKONTROL_MIN_BUILD 26200
!define TOTALKONTROL_WINDOWS_TOO_OLD "Cette version de totalKONTROL nécessite Windows 11 25H2 (build ${TOTALKONTROL_MIN_BUILD}) ou plus récent.$\r$\n$\r$\nSur ce Windows, installez la version standard de totalKONTROL, qui passe par loopMIDI."

!macro TOTALKONTROL_CHECK_WINDOWS
  ${IfNot} ${AtLeastBuild} ${TOTALKONTROL_MIN_BUILD}
    MessageBox MB_ICONSTOP "${TOTALKONTROL_WINDOWS_TOO_OLD}" /SD IDOK
    Abort
  ${EndIf}
!macroend

; Interactive installs: check as soon as the window opens, before any page.
!define MUI_CUSTOMFUNCTION_GUIINIT TotalKontrolCheckWindows
Function TotalKontrolCheckWindows
  !insertmacro TOTALKONTROL_CHECK_WINDOWS
FunctionEnd

; Silent installs (/S) skip .onGUIInit: check again before copying any file.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro TOTALKONTROL_CHECK_WINDOWS
!macroend
