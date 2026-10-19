# Teste do hook do instalador (Npcap)

Testa `src-tauri/windows/hooks.nsh` nos dois caminhos sem mexer no Npcap da máquina:

- `test-ausente`: a mesma macro, apontando para uma chave/DLL que não existem
  (`mk.py` gera `hooks-ausente.nsh`). Esperado: `npcap=[]` e o falso instalador chamado com
  `/winpcap_mode=yes`.
- `test-real`: a macro original. Esperado (com Npcap instalado): `npcap=[C:\Program Files\Npcap]`
  e o falso instalador não chamado.

```bash
cp ../../src-tauri/windows/hooks.nsh hooks-real.nsh && python -I mk.py
M="$LOCALAPPDATA/tauri/NSIS/makensis.exe"
"$M" fake-npcap.nsi && "$M" test-ausente.nsi && "$M" test-real.nsi
./hook-test-ausente.exe; cat "$TEMP/copilot-hook-test-ausente/result.txt"
./hook-test-real.exe;    cat "$TEMP/copilot-hook-test-real/result.txt"
```
