# T20 — Controlar backlog e adaptar qualidade à causa da lentidão

Prioridade: P2. Tipo: Core. Status: Em validação.
Dependências: T03, T04, T05, T06, T07, T18.

## Contexto

O adaptador atual compara RTT >150 ms ou FPS informado menor que 60% do enviado. Não distingue CPU, decoder, tela oculta e rede. O pipeline captura, codifica e escreve serialmente; filas no decoder/socket podem aumentar atraso.

## Implementação

1. Definir orçamento de idade de frame e memória por sessão com métricas T05. Documentar valores iniciais e como serão calibrados.
2. Separar produção de frame bruto, encode e escrita quando o benchmark justificar. Usar slot/fila pequena de frames brutos recentes; descartar antes de codificar é o caminho preferencial.
3. Limitar frames codificados em voo e bytes pendentes. Nunca implementar “drop oldest” genérico em H.264 sem considerar referências.
4. Ao invalidar cadeia codificada, interromper sequência, solicitar IDR e retomar com configuração da geração certa. Bytes já entregues ao TCP exigem drenagem limitada ou reconexão negociada, não remoção imaginária.
5. Priorizar mensagens de controle entre mensagens completas. Input PC-bound tem caminho próprio de leitura, mas não deve disputar mutex indefinidamente com envio.
6. Não deixar áudio.try_iter drenar sem orçamento se produtores mantiverem a fila alimentada; garantir fairness entre mídias/controle.
7. Classificar gargalo por tempo de captura/encode, fila de rede, decode/render e estado térmico/visibilidade.
8. Adaptar bitrate primeiro quando rede for o limite; FPS/resolução conforme restrição real. Respeitar perfil manual, limites T03 e preferências por sessão.
9. Usar histerese, cooldown, mínimo de amostras e recuperação gradual. Cena estática não prova capacidade para subir a qualidade; exigir evidência sob movimento.
10. Alterar bitrate dinamicamente onde o encoder suportar; caso contrário reconstruir somente o componente necessário, sem reiniciar monitor.
11. Exibir motivo “rede”, “encoder”, “tablet” ou “temperatura”; não chamar toda queda de rede lenta.

## Critérios de aceite

- [ ] Memória e idade de fila têm limite verificável sob throughput inferior ao bitrate.
- [ ] Ao voltar capacidade, vídeo converge para o presente sem longa reprodução atrasada.
- [ ] Recuperação não produz P-frames dependentes de quadros descartados.
- [ ] Duas sessões se adaptam independentemente.
- [ ] Perfil manual e pausa não entram em oscilações/rebuild desnecessários.

## Testes e rollout

Harness com rede lenta/perda/jitter, encoder lento, decoder lento, cena estática→movimento e controle contínuo. Não fazer shaping da rede global do usuário; usar proxy/link de teste.

Metas iniciais propostas: fila bruta ≤2 frames, nenhum crescimento ilimitado e recuperação de imagem recente em até 2 s no hardware de referência após congestionamento curto. Revisar números com benchmark e registrar tradeoffs.

Rollback desativa política automática nova mantendo limites/cancelamento. Não remover TLS nem a política de keyframes para melhorar uma medição.
