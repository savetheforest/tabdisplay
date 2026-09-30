# T21 — Comparar transportes e decidir se vale substituir o TCP atual

Prioridade: P3. Tipo: Spike opcional. Status: Em validação.
Dependências: T05, T06, T11, T17, T20.

## Pergunta

Após corrigir pacing, backpressure e decoder, o transporte ainda domina a latência nos cenários-alvo? Comparar TCP otimizado, canais separados, QUIC e WebRTC/RTP usando números, custo e compatibilidade.

## Trabalho

1. Usar a mesma cena, encoder, resolução, bitrate e aparelhos para todos os candidatos.
2. Medir LAN estável, Wi-Fi com perda/jitter, ADB e tethering. ADB reverse encaminha TCP; não supor que UDP/QUIC passa pelo mesmo reverse.
3. Distinguir QUIC stream confiável de datagram: só trocar a biblioteca não elimina todo bloqueio de entrega.
4. Avaliar criptografia, autenticação T11, handshake, priorização de controle, cancelamento e vínculo de canais à sessão.
5. Para datagrams/RTP, especificar MTU, fragmentação, sequência, reassembly com limites, deadlines, perda e pedido de IDR. Impedir alocação arbitrária por fragmentos incompletos.
6. Para WebRTC, avaliar tamanho/dependências do cliente Android, integração com MediaCodec e negociação; não introduzir servidor público STUN/TURN obrigatório para uso local.
7. Para canais separados, impedir que outro cliente injete mídia na sessão; usar autenticação derivada/associada ao canal seguro, não um session_id previsível.
8. Manter áudio, input e vídeo com políticas de confiabilidade diferentes; teclas/UP/controle permanecem confiáveis.
9. Definir escolha por transporte e fallback TCP seguro quando o candidato não funciona. Não reconectar indefinidamente alternando caminhos.
10. Registrar manutenção, licenças das dependências e suporte de plataformas, sem fazer compromisso comercial.

## Entrega e aceite

- [ ] Relatório comparativo com throughput, latência p95, perda, CPU e tamanho do binário.
- [ ] Diagrama de canais e contrato de autenticação/controle de recursos.
- [ ] Resultado por USB ADB, tethering e Wi-Fi, inclusive caminhos não suportados.
- [ ] Recomendação motivada: manter TCP, melhorar canalização ou criar tarefas de migração.
- [ ] Protótipo isolado não muda default nem remove compatibilidade existente.

Não implementar transporte criptográfico próprio nem abrir acesso à internet no roteador. NAT traversal remoto, relay pago e controle pela internet são novos escopos de produto.

## Testes do protótipo

Simular perda, duplicação, reordenação, fragmentação incompleta e peer que para de consumir. Tentar associar um canal de mídia à sessão errada e confirmar recusa. Comparar limite de memória, tempo de recuperação, ordem de DOWN/UP e latência de áudio. Repetir com fallback TCP e ADB para demonstrar que o experimento não tornou o cabo dependente de UDP.

Rollback: excluir/desativar somente o protótipo e manter benchmarks. “Sem ganho relevante” é conclusão válida.
