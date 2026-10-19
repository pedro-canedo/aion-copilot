Unicode true
!include LogicLib.nsh
!include FileFunc.nsh
!include x64.nsh
OutFile "hook-test-real.exe"
InstallDir "$TEMP\copilot-hook-test-real"
RequestExecutionLevel user
SilentInstall silent
!include "hooks-real.nsh"
Section
  SetOutPath "$INSTDIR\deps"
  File /oname=npcap-installer.exe "fake-npcap.exe"
  SetRegView 64
  !insertmacro NSIS_HOOK_POSTINSTALL
  FileOpen $9 "$INSTDIR\result.txt" w
  FileWrite $9 "npcap=[$0] rc=[$1]"
  FileClose $9
SectionEnd
