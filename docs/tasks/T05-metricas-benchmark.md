# T05 — Medir cada etapa e produzir benchmarks comparáveis

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01, T02, T03. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Contexto

Stats atualmente mostra FPS enviado, Mbps de vídeo, média de encode e RTT de aplicação. Stream.kt incrementa rendered mesmo quando releaseOutputBuffer falha; isso não comprova imagem apresentada. Sem separar etapas, trocar transporte pode esconder o verdadeiro gargalo.

## Arquivos

server.rs: Stats/stream_once/PING; tls.rs; win/capture.rs e encode.rs; mac/capture.rs e encode.rs; Stream.kt; desktop/src/main.js; MainActivity.kt; telemetry.rs e Crumbs.kt.

## Implementação

1. Definir métricas: captura, cópia, resize/conversão, espera do encoder, encode, espera de envio, bytes, idade/profundidade das filas, decode, submissão à Surface e recuperação de keyframe.
2. Contar frames únicos separadamente de repetições FLUSH, descartes e tentativas. Normalizar contadores pelo tempo real da janela, não presumir que todo PING chegou em exatamente um segundo.
3. Correlacionar frames/configurações com IDs negociados em T02. Timestamps monotônicos são locais; calcular RTT local e durações locais.
4. Não subtrair relógios do PC e tablet sem estimativa explícita de offset/erro. Para latência visual real, usar cena de referência e medição externa documentada quando disponível.
5. Usar listener de renderização como evidência melhor que releaseOutputBuffer, registrando limitações do callback e sem equipará-lo automaticamente à emissão de luz do painel.
6. Agregar p50/p95/p99 e amostras perdidas em janelas limitadas. Amostragem/contadores não podem alocar ou persistir um log por frame indefinidamente.
7. Criar painel por sessão que mostre codec, encoder, resolução, FPS alvo/efetivo e meio conhecido da conexão.
8. Criar exportação local explícita de relatório sanitizado, com versão/build, hardware genérico e métricas. Captura de tela, tokens, nome/IP e serial ficam fora por padrão.
9. Definir roteiro de benchmark: release, aquecimento, cena estática, scroll/texto, vídeo/movimento, 60 s de medição e três repetições, sempre com condições registradas.
10. Comparar instrumentação ligada/desligada e limitar sua sobrecarga. Propor orçamento a partir dos dados, sem inventar ganho percentual.

## Critérios de aceite

- [ ] Contadores não aumentam por falha de release nem por callback de geração antiga.
- [ ] Tela estática é identificada como ociosa, não “rede ruim porque FPS=0”.
- [ ] Gráfico separa RTT de latência de frame e distingue FPS enviado/decode/render observado.
- [ ] Relatório não contém conteúdo/identificadores privados sem escolha explícita.
- [ ] Benchmark permite reproduzir a comparação com mesmos parâmetros.

## Testes e rollback

Usar relógio falso para PINGs atrasados, intervalos diferentes, frames repetidos e counter reset. Inspecionar relatório com strings-sentinela representando segredos. Executar benchmark antes/depois de T04 quando ambas estiverem disponíveis.

Rollback desativa coleta detalhada e mantém estatísticas essenciais; não altera protocolo sem negociar a ausência dos campos.
