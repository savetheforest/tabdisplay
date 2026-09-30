# T09 — Dar ownership estável à sessão e corrigir reconexão/lifecycle

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01, T06, T07. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problema

Stream pertence à Activity; o serviço só mantém notificação/wake lock. onDestroy fecha Stream apenas se isFinishing. Uma recriação por mudança não coberta por configChanges pode perder a referência e deixar callbacks na Activity antiga. Reconexão depende do discovery e onPaired não atualiza last_pc da primeira conexão manual.

## Implementação

1. Criar um dono da sessão fora da Activity, preferencialmente serviço vinculado/controlador com lifecycle explícito. Activity observa estado e fornece Surface, sem ficar retida por callbacks.
2. Definir máquina de estados de conectar, parear, ativo, pausado, retry e encerrado, com session_id para descartar resultados de tentativa anterior.
3. Unificar disconnect/erro/timeout/serviço parado em cleanup idempotente. Notificação deve refletir estado e oferecer Desconectar.
4. Reavaliar foregroundServiceType com documentação oficial. O target é 36, dataSync tem timeout nas condições de Android 15+. Avaliar connectedDevice e seus pré-requisitos; registrar a escolha, não trocar tipo apenas para escapar de limite.
5. Se houver tipo com timeout, implementar encerramento apropriado. Não iniciar de background em situação proibida nem esconder falhas de startForegroundService.
6. Pedir POST_NOTIFICATIONS no contexto adequado e tratar recusa. Recusa não equivale automaticamente à impossibilidade de todo foreground service; validar comportamento por API.
7. Wake lock só durante necessidade real, adquirido/liberado de forma balanceada; onDestroy/onTimeout/cancelamento limpam recursos.
8. Reconnect com backoff limitado, relógio monotônico, botão cancelar e motivo categorizado. Não repetir automaticamente erro de identidade, incompatibilidade ou credencial revogada.
9. Após pareamento manual, persistir pc_id/token/endpoints coerentes para futuras reconexões; adaptar descoberta de IP alterado.
10. Tratar descoberta tardia e Runnable antigo: uma tentativa obsoleta não troca o PC escolhido manualmente.
11. Process death encerra a conexão; restaurar preferências/intenção de forma segura, sem prometer sessão intacta nem manter Surface velha.

## Critérios de aceite

- [ ] Recriar Activity mantém no máximo uma sessão e nenhuma referência à Activity destruída.
- [ ] Falha/cancelamento libera serviço, socket, codec, áudio e wake lock.
- [ ] Reconexão manual pareada funciona após mudança de IP via descoberta autenticada.
- [ ] Revogação/mismatch não gera loop de novo pareamento automático.
- [ ] Android 11–16 têm comportamento documentado, incluindo notificações negadas e background.

## Validação

“Não manter atividades”, mudança de tema/idioma/tamanho de fonte, lock, split-screen, rotação, morte de processo e desconexão durante diálogo. Testar timeout em emulador/aparelho de teste usando mecanismos oficiais; registrar e restaurar eventuais ajustes de teste.

Fontes: [tipos de serviço](https://developer.android.com/develop/background-work/services/fgs/service-types) e [timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout).

Rollback: preservar novo ownership/cancelamento como unidade; migrar preferências com leitura retrocompatível, sem apagar pareamentos.
