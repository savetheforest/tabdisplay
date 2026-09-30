# T12 — Tornar a conexão ADB previsível e suportar vários tablets

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01, T11. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Contexto

lib.rs executa reverse e install sem serial, a cada três segundos no caso de reverse. Com vários dispositivos, ADB pode recusar por ambiguidade. A UI reduz todos os erros a ausência de cabo; Mac não inclui adb no empacotamento inspecionado.

## Implementação

1. Extrair um gerenciador ADB com runner de processo testável, argumentos separados, timeout, captura sanitizada de stdout/stderr e cancelamento.
2. Usar acompanhamento de dispositivos suportado pela versão detectada. Se track-devices falhar, reiniciar com backoff e polling limitado; verificar capacidades em vez de assumir variante do comando.
3. Manter estados por serial: ausente, unauthorized, offline, disponível, reverse ativo e erro. Distinguir transporte USB de ADB via rede quando houver evidência confiável.
4. Executar reverse direcionado por serial/transport-id; manter apenas o mapeamento da porta do TabDisplay em cada aparelho.
5. Verificar sucesso do reverse e reestabelecer após reconnect. Não remover forwards/reverses de outros aplicativos.
6. No botão de instalar, selecionar o aparelho explicitamente quando houver mais de um. Informar instalação versus atualização, incompatibilidade de assinatura e autorização pendente.
7. Não desinstalar para contornar assinatura diferente: isso apaga dados e depende de decisão do usuário. Não instalar APK em todos os devices por padrão.
8. Escolher ADB do PATH ou bundled de forma determinística, verificar execução/versão e prever fallback. Não usar adb kill-server como recuperação rotineira: afeta Android Studio e outras sessões.
9. Rodar processos sem janelas de console no Windows e sem bloquear o runtime/UI. Limitar tamanho de saída e tratar path com espaços.
10. Integrar bootstrap/autenticação de T11; reverse não equivale a identidade confiável.
11. Documentar decisão Mac: distribuir binário verificado adequado ou informar instalação necessária. Coordenação de empacotamento fica em T24.

## Critérios de aceite

- [ ] Dois tablets USB conectam ao mesmo PC sem erro de seleção ambígua.
- [ ] Instalação afeta somente o aparelho escolhido.
- [ ] unauthorized/offline/ADB ausente têm mensagens e passos distintos.
- [ ] Troca de cabo/dispositivo não exige reiniciar o app.
- [ ] ADB compartilhado com Android Studio não é encerrado.

## Testes e rollback

Fake de processo com listas de 0/1/2 devices, serial com caracteres inesperados, timeout, saída inválida, stderr e versão incompatível. Teste físico com dois devices e autorização revogada; testar apenas aparelhos designados para isso.

Fonte: [manual oficial do ADB](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/docs/user/adb.1.md). Confirmar sintaxe/capacidades da versão escolhida.

Rollback retorna a polling direcionado por serial, nunca ao comando ambíguo nem à remoção global de mapeamentos.
