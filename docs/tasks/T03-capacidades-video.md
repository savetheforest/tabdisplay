# T03 — Negociar um modo de vídeo que o decoder selecionado suporte

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T02. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problema e objetivo

decodableSize consulta caps.any a 60 FPS, newDecoder escolhe pelo MIME, e Custom aceita 90/120. O decoder escolhido pode não ser o que aprovou o tamanho. Negociar combinações, não apenas adicionar um max_fps global.

## Arquivos e contratos

MainActivity.kt: decodableSize/HELLO; Stream.kt: newDecoder/CONFIG; server.rs: Hello/plan/CONFIG; settings.rs; desktop/src/main.js e index.html. Consultar as APIs de codecs em REFERENCIAS.md.

## Implementação

1. Enumerar decoders AVC utilizáveis, com nomes canônicos, hardware/software, perfil/level e suporte às features necessárias. Ignorar codecs secure/tunneled que exijam fluxo não implementado.
2. Selecionar um decoder concreto e criá-lo pelo nome. Capabilities e runtime devem se referir à mesma implementação.
3. Calcular uma lista finita de modos por orientação: resolução, FPS, alinhamento e restrições. Usar performance points/estimativas quando disponíveis sem interpretar null como falha.
4. Respeitar taxa da tela, capacidade anunciada e capacidade do encoder. A Surface pode sugerir frame rate suportado pela API, sem prometer que o sistema o aplicará.
5. Enviar capacidades negociadas e receber CONFIG com tamanho/taxa efetivos e motivo de redução. O PC nunca envia um modo além da interseção suportada.
6. Ao girar, recomputar a combinação; não assumir que trocar largura e altura sempre é aceito. Evitar laço de renegociação provocado pelo letterbox.
7. Configurar operating-rate conforme modo efetivo. KEY_LOW_LATENCY é best effort condicionado a suporte; prever tentativa de configuração sem a opção quando for a causa da rejeição.
8. Se configure/start falhar, tentar modos/decoder de fallback com limite e erro final explícito; reportar a capacidade realmente escolhida.
9. Atualizar a UI para opções disponíveis por tablet. Preservar intenção do usuário separada do modo efetivo.
10. Definir defaults conservadores para clientes v3 que só anunciam decodable.

## Cuidados

- Não chamar codecs de hardware “garantidamente 120 FPS” só por areSizeAndRateSupported.
- Não rebaixar minSdk nem pedir permissão nova sem necessidade.
- Não fazer uma sequência cara de consultas/configure na thread principal.
- Não aumentar a resolução física do monitor espelhado para atender perfil; apenas dimensionar o stream.

## Critérios de aceite

- [ ] Decoder selecionado e capacidade anunciada são coerentes.
- [ ] Tablet que só suporta o modo a 60 não recebe esse modo a 90/120.
- [ ] 30/60/90/120 aparecem apenas onde houver combinação válida; há fallback explicado.
- [ ] Retrato e landscape são testados sem ciclos de resize.
- [ ] Usuário distingue resolução da tela, do monitor e do vídeo.

## Testes e reversão

Testes puros com dois decoders de capacidades diferentes, ausência de performance points, alinhamento incomum e taxa da tela inferior à solicitada. Teste físico em ao menos um MediaTek/Qualcomm ou registrar a plataforma faltante. Incluir configuração falhando e fallback funcionando.

Rollback usa a negociação legada compatível, limitado ao modo validado; não voltar a enviar 120 por confiança no slider.
