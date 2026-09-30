# T11 — Vincular identidade, canal e autorização de pareamento

Prioridade: P0. Tipo: Core. Status: Em validação.
Dependências: T01, T02, T06. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problemas e fronteiras

onServerCertificate salva pin antes do pareamento e remove pin/token após mudança de certificado. O próximo retry pode aceitar identidade nova. server.rs dispensa pareamento se peer é loopback, o que não comprova USB físico. UDP informa pc_id sem autenticação.

## Arquivos

MainActivity.kt: onServerCertificate/onPaired/connect; Stream.kt: secure; Discovery.kt; server.rs: handle/pair/register; pairing.rs; tls.rs; lib.rs: forget_device; armazenamento Android.

## Implementação em etapas

1. Escrever modelo de ameaça: LAN hostil, beacon falso, primeiro contato MITM, certificado alterado, app local no tablet/PC, ADB sem fio, token roubado/revogado.
2. Separar endpoint descoberto de identidade confiável. O pc_id recebido em broadcast não autoriza token nem substitui o pin existente.
3. Usar fingerprint candidato somente em memória até confirmação apropriada. Em mismatch, preservar a confiança anterior e exigir fluxo explícito de reidentificação; suspender retry automático.
4. Projetar prova de identidade vinculada ao canal: comparar dado derivado da sessão/certificado nas duas telas, QR com fingerprint verificado ou protocolo de pareamento revisado. Documentar qual ataque o método cobre.
5. Não alegar proteção MITM apenas por mover a gravação do pin depois de digitar o mesmo código em um canal que o atacante pode retransmitir. Não implementar PAKE/criptografia artesanal.
6. Para loopback, preferir autenticação consistente ou bootstrap efêmero vinculado a device/serial comprovado. Preservar boa UX USB sem confiança irrestrita na porta local.
7. Persistir identidade, pin, pc_id e token de forma coerente; migrar pin por IP/127.0.0.1 para identidade de PC, tratando dois PCs usados pelo mesmo cabo.
8. Fortalecer tentativas: limite por sessão e janela temporal, expiração com relógio monotônico e proteção contra reiniciar conexão para obter tentativas ilimitadas. Evitar bloqueio global indefinido por um cliente.
9. Revogar token e desconectar sessões correspondentes quando o usuário escolher esquecer/revogar. Separar isso de excluir um endpoint da lista.
10. Proteger arquivos de chave/token por permissões do SO e política de backup Android; considerar armazenamento seguro sem perder migração/reinstalação. Não imprimir material secreto.
11. Tratar certificados/pares de arquivos corrompidos com erro recuperável e procedimento de regeneração explícito; não panic nem rotação silenciosa.

## Critérios de aceite

- [ ] Beacon falso não causa envio de token a identidade divergente.
- [ ] Mismatch não apaga confiança nem dispara aceitação automática no retry.
- [ ] Primeiro contato tem garantia e limitações documentadas; nenhuma alegação criptográfica sem teste/modelo.
- [ ] Dois PCs USB podem ser pareados sem sobrescrever confiança um do outro.
- [ ] Revogação encerra autorização ativa e bloqueia reconexão antiga.
- [ ] Falha de gravação não mostra pareamento persistido com sucesso.

## Testes e rollback

Harness com PCs A/B e certificados distintos; replay de pc_id; mudança de IP legítima; token inválido/revogado; cancelamento; certificado corrompido; limite de tentativas; conexão local não autorizada. Para MITM, usar somente laboratório controlado.

Migração deve preservar dados anteriores até concluir. Rollback não pode restaurar uma regra de autoaceitação de identidade nova; se necessário, exigir re-pareamento seguro com mensagem clara.
