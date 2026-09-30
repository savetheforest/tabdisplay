# T31 — Avaliar um cliente iOS/iPadOS

Prioridade: P3. Tipo: Spike opcional. Status: Concluída.
Dependências: T02, T03, T05, T11, T13.

## Objetivo

Definir viabilidade técnica e de distribuição de um cliente local para iPad/iPhone. Não existe cliente iOS no repositório; isto não é uma tarefa de portar arquivos Kotlin mecanicamente.

## Investigação

1. Avaliar cliente nativo Swift com VideoToolbox/renderização, Network framework e áudio, mantendo contratos de T02/T11.
2. Prototipar Wi-Fi/LAN primeiro: descoberta suportada, permissão de rede local, pareamento e pinning.
3. Determinar dimensões/FPS/codec por aparelho e Surface/render equivalente; aplicar mesma distinção entre capacidade nominal e desempenho sustentado.
4. Mapear touch, Pencil, pressão/inclinação/hover conforme modelo/API; não anunciar recursos em dispositivos que não os forneçam.
5. Definir lifecycle, background, lock, multitarefa/Stage Manager e orientação; não prometer serviço contínuo igual ao Android.
6. Investigar USB separadamente em documentação Apple. ADB/AOA do Android não são caminhos disponíveis por inferência; acessórios, entitlements e distribuição podem impor restrições.
7. Medir impacto no protocolo, tamanho do app, certificados de desenvolvimento e ambiente Mac necessário.
8. Especificar testes e estratégia de compatibilidade com desktop legado/novo.
9. Documentar dependências de conta, hardware e submissão, sem criar conta nem assumir autorização de publicação.

## Aceite

- [ ] Arquitetura candidata, lista de capacidades e limitações por dispositivo.
- [ ] LAN e USB têm avaliações separadas e baseadas em fontes oficiais.
- [ ] Protótipo, se executado, contém teste de vídeo/input em iPad real e benchmark.
- [ ] Plano de implementação dividido em conexão, mídia, input, UX e distribuição.
- [ ] Decisão de continuar ou adiar, com custos/pendências concretos.

Rollback não muda desktop padrão; features adicionais só são anunciadas por negociação. Sem Mac/iPad, manter a parte experimental pendente.
