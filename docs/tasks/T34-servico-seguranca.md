# T34 — Limitar a superfície do serviço privilegiado e do servidor local

Prioridade: P0. Tipo: Core. Status: Em validação.
Dependências: T01, T02.

## Contexto e limites da evidência

win/service.rs atende pipe com threads por conexão, leitura lines sem teto e comandos de modos/restart. O serviço altera driver como processo privilegiado. Isso exige limites e identidade bem definidos; a inspeção não demonstrou uma exploração concreta de elevação de privilégio.

## Arquivos

win/service.rs: SDDL/serve/client/handle/release; win/driver.rs; win/display.rs: ensure_modes; windows/hooks.nsh; lib.rs: comandos Tauri; capabilities/default.json; tauri.conf.json; server.rs: run.

## Implementação

1. Documentar quais usuários/processos devem acessar o serviço e quais operações são permitidas. Revisar SDDL e identidade de cliente efetiva.
2. Restringir pipe a clientes locais quando aplicável e não expor controle do driver por rede. Nome conhecido de pipe não é autenticação.
3. Limitar conexões/threads, bytes por linha, comandos por segundo e tempo sem progresso. Rejeitar linha longa sem esperar alocação indefinida. O servidor usa `PIPE_NOWAIT` e timeout de 5 s desde o último byte, além do teto de 256 bytes.
4. Validar comando completo e dimensões antes de tocar disco/driver; rejeitar tokens extras e overflow.
5. Evitar I/O de driver prolongado segurando locks globais sem cancelamento/estado observável. Serializar mudanças de topologia numa política clara.
6. Garantir contagem de leases por conexão e cleanup exatamente uma vez, inclusive falha de resposta, processo cliente morto e serviço parado.
7. Dar orçamento a modos XML e gravar com backup/validação; preservar dados não pertencentes ao TabDisplay. Rejeitar path controlado pelo cliente.
8. Verificar ACLs do diretório/configuração/binário usados pelo serviço e caminhos com reparse points. Não permitir que usuário sem privilégio substitua executável/configuração interpretada com poder adicional.
9. Revisar comandos privilegiados de instalação/desinstalação e propriedade do driver, preservando driver preexistente.
10. Revisar CSP/capabilities do Tauri: a configuração atual tem csp null. Introduzir restrição compatível com recursos locais e remover permissões não usadas; não quebrar IPC com política copiada sem teste.
11. Validar dados externos mostrados no frontend: usar textContent para nome/erro; evitar inserir payload de rede em innerHTML. O diagnóstico dinâmico do guia deve entrar como texto mesmo quando o template visual for estático.
12. Coordenar com T06 limites de conexão de rede antes de autenticação; MAX_TABLETS não limita threads de handshake por si só.

## Critérios de aceite

- [ ] Cliente local fora da política não consegue controlar o driver.
- [ ] Input gigante/lento e múltiplas conexões não consomem recursos ilimitados.
- [ ] Encerramento libera leases e não desliga monitor ainda usado por outro cliente.
- [ ] Configurações/modos inválidos não chegam à API privilegiada.
- [ ] Frontend mantém funcionalidade com permissões/CSP justificadas.

## Testes e reversão

Testes de parser com fakes de driver, ACL/pipes em VM de teste e cliente que nunca termina linha. Testar interrupção do serviço em ambiente designado, nunca no desktop em uso sem autorização.

Relatar achados como confirmados/reproduzidos/hipóteses. Rollback preserva validação e restrição local; não restaurar permissões amplas como conserto automático de compatibilidade.
