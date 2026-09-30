# Relatório T26 — Teclado, trackpad e atalhos

Data: 29/09/2026  
Status: Em validação

Foi adicionado o canal v3 opcional `KEY` (20), separado do `INPUT`: texto commitado pelo IME usa UTF-8 limitado a 16 KiB; teclas usam nomes lógicos allow-listados e quatro modificadores, sem Android keycodes, scancodes recebidos ou macros.

O tablet anuncia `keyboard` no HELLO e oferece uma ação explícita “Teclado” com campo IME, Ctrl/Shift/Alt e teclas especiais. O PC só processa KEY se a capability e “Controlar o PC” estiverem ativas. Windows injeta texto via Unicode e teclas lógicas com liberação de modificadores; macOS tem o adaptador CGEvent correspondente. Não há shell, macro recebida ou execução arbitrária.

Evidência física: APK release reinstalado sem limpar dados; menu/diálogo “Teclado” visíveis no Redmi Pad 2; `T26_SENTINEL_20260929` foi enviado do campo IME para uma janela Notepad temporária sem derrubar a sessão. Testes pendentes: emoji/fora do BMP em aplicativo real, ABNT2/layout alternativo, dead keys, teclado externo, trackpad, queda durante tecla pressionada e matriz Mac. Nenhuma tecla pode ser injetada sem capability e sem “Controlar o PC”.
