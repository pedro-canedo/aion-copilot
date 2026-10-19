; aion-copilot: installs what the meter needs on a fresh PC.
;
; WebView2 comes from Tauri's embedded bootstrapper (tauri.conf.json).
; Npcap's free license does not allow shipping its installer inside ours, so
; when it is missing the player is sent to npcap.com to download it (the
; defaults there already include WinPcap API-compatible mode).
;
; Npcap writes HKLM\SOFTWARE\Npcap in the 32-bit registry view (WOW6432Node),
; while this x64 installer reads the 64-bit view: look in both, and for the
; DLL itself, before deciding it is missing.

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $0 ""
  SetRegView 32
  ReadRegStr $0 HKLM "SOFTWARE\Npcap" ""
  SetRegView 64
  ${If} $0 == ""
    ReadRegStr $0 HKLM "SOFTWARE\Npcap" ""
  ${EndIf}
  ${If} $0 == ""
  ${AndIf} ${FileExists} "$WINDIR\System32\Npcap\wpcap.dll"
    StrCpy $0 "$WINDIR\System32\Npcap"
  ${EndIf}
  ${If} $0 == ""
  ${AndIf} ${FileExists} "$WINDIR\Sysnative\Npcap\wpcap.dll"
    StrCpy $0 "$WINDIR\Sysnative\Npcap"
  ${EndIf}
  ${If} $0 == ""
    MessageBox MB_OK|MB_ICONINFORMATION "O aion-copilot usa o Npcap para ler o tráfego do AION 2, e ele ainda não está instalado neste PC.$\r$\n$\r$\nA página de download do Npcap vai abrir agora: baixe o instalador, deixe as opções como estão (WinPcap API-compatible Mode marcado) e clique em Install. Depois abra o aion-copilot." /SD IDOK
    ExecShell "open" "https://npcap.com/#download"
  ${EndIf}
!macroend
