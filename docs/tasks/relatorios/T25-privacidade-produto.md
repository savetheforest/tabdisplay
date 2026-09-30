# Relatório T25 — Privacidade, produto e documentação

Data: 29/09/2026  
Status: Em validação

## Entrega implementada

- README, PROTOCOL, guia de primeira execução, strings Android, política e ficha Play foram comparados com o comportamento v3 atual.
- Removida a promessa de que “o cabo basta” ou dispensa pareamento: ADB loopback e rede USB também usam token/certificado por PC; a UI chama o caso local de ADB e não prova cabo físico.
- A ficha Play foi alinhada ao `connectedDevice` real do Android, em vez do antigo `dataSync`.
- A política mantém variantes sem Sentry (padrão) e com Sentry opt-in de build separadas, descrevendo breadcrumbs técnicos e retenção local privada.
- Breadcrumbs Android passam por redaction de tokens/códigos/senhas e IPv4 antes de Sentry ou do export local; o export retém no máximo 64 eventos e não inclui mídia de tela.
- Diagnósticos desktop usam sanitização de caminhos/endpoints comuns antes de log/Sentry. O teste de telemetria cobre caminho Windows e endereço sentinela.
- O placeholder de contato foi mantido como bloqueio explícito de publicação, sem inventar e-mail, preço ou loja.

## Arquivos principais

- `README.md`
- `PROTOCOL.md`
- `docs/privacy.md`
- `docs/play-store/ficha.md`
- `desktop/src/main.js`
- `desktop/src-tauri/src/telemetry.rs`
- `android/app/src/main/java/com/tabdisplay/Crumbs.kt`
- `android/app/src/test/java/com/tabdisplay/CrumbsTest.kt`

## Validação local

- Teste Rust de sanitização adicionado à suíte de telemetria.
- Teste JVM de export Android cobre sentinela de token e endereço IPv4.
- `testDebugUnitTest assembleDebug` passou após o novo teste de redaction (44 tarefas).

## Pendências

Não há endpoint real de produção, contato de distribuição, consentimento Play configurado, payload remoto Sentry de produção ou publicação autorizada nesta sessão. Falta revisar políticas oficiais da loja no momento de submissão e substituir o placeholder antes de publicar. Screenshots são material de referência e precisam ser refeitas/confirmadas com estado atual real.
