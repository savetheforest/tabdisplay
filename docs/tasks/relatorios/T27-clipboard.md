# Relatório T27 — Clipboard

Data: 29/09/2026  
Status: Em validação

Implementado o MVP no protocolo v3 como capability opcional `text_transfer` e mensagem `TEXT` (19). O tablet e o PC anunciam a capability somente nos clientes atualizados; clientes antigos continuam com o recurso desligado. O limite é 16 KiB de UTF-8 por texto, com JSON versionado e direção (`tablet`/`pc`) validada.

As ações são explícitas: no tablet, “Enviar texto copiado” lê o item primário somente após o toque; no PC, “Enviar texto copiado” lê a área de transferência somente após o clique. O receptor exibe confirmação e só copia ao clipboard após “Copiar”. Não há leitura em background, serviço de acessibilidade, colagem automática, abertura de URL, histórico, persistência ou telemetria do conteúdo. O inbox do PC é memória transitória e guarda no máximo o último texto.

Testes automatizados cobrem framing, Unicode, direção, limite de 16 KiB e descarte do inbox ao terminar a sessão; `cargo test --locked` passou com 48 testes unitários, 1 ignorado e 1 integração em 29/09/2026. O APK foi reinstalado fisicamente com `adb install -r` preservando dados, a sessão USB voltou com SurfaceView e o menu mostrou “Enviar texto copiado”; a confirmação física do fluxo de conteúdo nos dois sentidos ainda é pendente, assim como revogação/background e duas sessões. O pareamento de vídeo não autoriza clipboard implicitamente.

Correção adicional: o `TEXT_INBOX` agora é associado ao `session_id` e é descartado no `Drop` da sessão. Isso impede que um diálogo de confirmação revele texto recebido por uma sessão já revogada ou desconectada. O release foi recompilado, o processo do workspace reiniciado, e a sessão voltou a funcionar por ADB reverse USB; o tablet exibiu SurfaceView ativo (~44 fps) com o Wi‑Fi desligado, sem limpar dados.

Tentativa adicional de conteúdo físico: o ADB bundled deste aparelho não implementa `cmd clipboard`, e o campo Compose de teste não expôs seleção/cópia confiável por UIAutomator. Um texto sintético foi usado somente em rascunho local e removido; nenhum fluxo de clipboard foi contado como aprovado. O TabDisplay foi restaurado ao primeiro plano, com Wi‑Fi ligado e SurfaceView ativo.
