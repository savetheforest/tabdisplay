# T25 — Alinhar documentação, privacidade e promessas ao comportamento

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T05, T11, T22, T24.

## Contexto

README/PROTOCOL têm parte do comportamento descrito; comentários antigos mencionam plaintext/v2. Material da loja diz que todo cabo dispensa código, embora tethering já use pareamento. Política tem contato placeholder; link de compra aponta para o repositório. Sentry é opt-in de build, não necessariamente escolha disponível ao usuário.

## Implementação

1. Auditar README, PROTOCOL.md, docs/privacy.md, docs/play-store/ficha.md e strings desktop/mobile contra recursos realmente entregues.
2. Corrigir termos USB ADB/rede USB/Wi-Fi/internet, requisitos, plataformas e limitações de caneta/gestos Mac.
3. Explicar coleta por variante e comportamento de updater/Sentry. “Nada sai do aparelho” não vale se uma build envia diagnósticos.
4. Inspecionar payloads reais de teste do Sentry: exceções, breadcrumbs, contexts e anexos; remover IP, paths pessoais, tokens, códigos, clipboard e conteúdo. send_default_pii=false sozinho não sanitiza texto arbitrário.
5. Definir controle de consentimento/configuração visível compatível com a distribuição escolhida, sem habilitar telemetria silenciosamente.
6. Documentar retenção/exclusão local, backup Android, revogação e certificado alterado segundo T11.
7. Substituir placeholders somente com informações reais fornecidas/configuradas. Se contato/loja não existir, registrar pendência ou esconder ação incompleta; não inventar email/preço.
8. Rever políticas vigentes da loja com fontes oficiais no momento de submissão; não tratar o checklist antigo como aprovação.
9. Atualizar screenshots somente com estado atual e dados de teste. Identificar simulação quando não for tela real.
10. Produzir changelog e guia de diagnóstico offline com termos compreensíveis e limites reais.

## Critérios de aceite

- [ ] Documentos não prometem recursos/plataformas não testados.
- [ ] Política de privacidade descreve precisamente cada variante de build.
- [ ] Payload de diagnóstico inspecionado não expõe sentinelas privadas.
- [ ] Usuário entende modos de conexão e comportamento de pareamento.
- [ ] Placeholders restantes aparecem como bloqueios de publicação, não conteúdo pronto.
- [ ] Nenhuma regra de licença foi afrouxada para a documentação parecer completa.

## Verificação e rollback

Busca por v2/plaintext/“cabo basta”/placeholders, revisão manual de links e textos, captura de telemetria apenas contra endpoint local de teste.

Mudanças documentais podem ser revertidas separadamente, mas alegações falsas devem continuar removidas. Hospedagem da política, compra e publicação em loja não fazem parte da simples edição destes arquivos.
