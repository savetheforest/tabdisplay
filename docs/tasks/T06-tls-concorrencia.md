# T06 — Evitar bloqueios em TLS e encerrar conexões de forma limitada

Prioridade: P0. Tipo: Core. Status: Em validação.
Dependências: T01, T02. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problema

tls.rs afirma não segurar mutex em I/O bloqueante, mas Write::write, respostas TLS em Read::read e shutdown chamam write_tls sob o mutex. Não há timeout de escrita. Um peer que não lê pode bloquear envio, leitura de input e encerramento; register ainda pode chamar shutdown enquanto segura SESSIONS.

## Arquivos

tls.rs: Conn/Read/Write/shutdown; server.rs: run/register/handle/read_msg; Stream.kt: send/run/close.

## Implementação

1. Reproduzir com sockets de teste: peer faz handshake e para de ler; outro para após HELLO parcial; um envia lentamente bytes para estender timeouts.
2. Definir dono do estado TLS e da escrita. Escolher I/O dedicado/event loop ou extração serializada de records; documentar a escolha e a ordem dos records.
3. Não liberar o mutex simplesmente permitindo dois escritores no socket. Records TLS e framing devem continuar ordenados, incluindo key updates e close_notify.
4. Implementar deadline total de handshake/HELLO/pareamento, limites de conexões pendentes e timeout/cancelamento de escrita. Timeouts por read não bastam contra um cliente que manda um byte a cada intervalo.
5. Implementar heartbeat com último progresso válido e encerramento de sessão sem PONG; distinguir stream ocioso de conexão morta. Pausa de vídeo mantém o canal de controle.
6. Remover encerramento de socket de dentro de locks globais: retirar/referenciar sessão, liberar lock, então cancelar.
7. Fazer shutdown idempotente e limitado. close_notify é best effort; um peer travado não pode impedir fechamento do TCP.
8. No Android, falha de envio deve cancelar a sessão e avisar uma vez; não engolir exceções. Evitar fila ilimitada de tarefas de escrita.
9. Registrar origem de erro sanitizada e preservar motivo enviado pelo servidor. Não deixar ERROR chegar seguido de EOF que apague a mensagem útil.
10. Garantir liberação de capture, encoder, áudio, injector e registro da sessão mesmo quando um worker falha.

## Critérios de aceite

- [ ] Peer que não lê não bloqueia status, novas sessões nem desconexão.
- [ ] Framing e handshake duplex continuam corretos com records fragmentados.
- [ ] Sessão sem progresso é encerrada dentro de prazo configurado e testável.
- [ ] Cancelar duas vezes não trava nem duplica callbacks.
- [ ] Threads/filas/memória não crescem após ciclos de conexões incompletas.

## Testes e limites

Testar carga grande, buffers pequenos, half-close, EOF durante payload, escrita parcial, key update, cancelamento simultâneo e reconnect com mesmo device_id. Usar portas efêmeras e certificado de teste. Medir duração e número de workers antes/depois de ao menos 100 ciclos.

Não trocar rustls, remover TLS ou migrar para UDP nesta tarefa. Rollback mantém timeouts/limites seguros; arquitetura alternativa só integra se os testes de cancelamento passarem.
