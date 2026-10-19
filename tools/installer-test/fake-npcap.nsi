Unicode true
!include FileFunc.nsh
OutFile "fake-npcap.exe"
RequestExecutionLevel user
SilentInstall silent
Section
  ${GetParameters} $0
  FileOpen $1 "$EXEDIR\fake-called.txt" w
  FileWrite $1 "args=$0"
  FileClose $1
SectionEnd
