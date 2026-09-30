# Guia obrigatório para executar uma tarefa

## Antes de alterar

1. Leia as instruções locais vigentes, este guia, a tarefa e suas dependências.
2. Registre commit, sistema operacional, versões das ferramentas e alterações locais. Preserve alterações do usuário e trabalhos de outras tarefas.
3. Abra os arquivos citados e siga os símbolos, não números de linha antigos. Referências de código neste pacote partem da raiz do repositório.
4. Confirme o problema por teste, cenário reproduzível ou cadeia de chamadas. Uma hipótese de desempenho exige medição; não declare um benchmark que não executou.
5. Escreva um plano curto com interfaces afetadas, compatibilidade, validação e rollback. Decisões reversíveis de implementação podem ser tomadas dentro do escopo.
6. Consulte as fontes oficiais na versão da API utilizada se houver dúvida. Não copie pseudocódigo como se fosse código de produção.

## Invariantes do produto

- Streaming é local. Wi-Fi local, Ethernet e rede USB podem funcionar sem internet. Atualizador e Sentry são tráfego externo separado.
- TCP não equivale a internet. A permissão INTERNET do Android continua necessária para sockets locais.
- Endereço loopback não comprova cabo físico. ADB também pode usar transporte sem fio; um processo local pode alcançar a porta.
- Preservar criptografia e autenticação. Não resolver falhas com certificados aceitos incondicionalmente, downgrade silencioso ou remoção do pareamento.
- Não registrar tokens, códigos, conteúdo da tela, áudio, clipboard, nomes pessoais ou identificadores estáveis nos relatórios remotos.
- Sem licença válida, Estender continua indisponível. Testes usam dependências/credenciais de teste; não modificar a chave pública para liberar produção.
- O suporte atual é Android API 30+, Windows e macOS 14+ conforme o projeto. Linux/iOS são expansões, não recursos existentes.
- Não reiniciar o driver para toda mudança de qualidade. Evitar RELOAD_DRIVER/SETDISPLAYCOUNT no driver 25.7 usado pelo projeto; há histórico documentado de Código 43.
- Recursos compartilhados precisam de dono, prazo de encerramento e liberação idempotente.

## Contratos de mídia e concorrência

- H.264 usa referências entre quadros. Pode-se substituir um frame bruto antes de codificar; descartar arbitrariamente um P-frame já codificado pode invalidar os seguintes.
- Depois de perda/descarte que invalida a cadeia, pedir IDR, transmitir configuração/SPS/PPS adequados e retomar apenas numa geração coerente.
- Bytes já escritos no TCP não podem ser retirados da fila pela aplicação. Uma fila em memória limitada não resolve, sozinha, backlog no socket.
- Não truncar uma mensagem parcialmente enviada e continuar a seguinte na mesma conexão. Isso quebra o framing.
- Um escritor por fluxo ou serialização explícita. Filas têm limites de quantidade, bytes e/ou idade, com política por tipo de mensagem.
- DOWN/UP/CANCEL, teclas e configuração não podem sumir para aliviar congestionamento. MOVE pode ser condensado apenas respeitando a sequência de cada contato.
- Não manter mutex global/TLS bloqueado em I/O potencialmente indefinido. Cancelamento deve funcionar com peer que parou de ler.
- Não somar timestamps absolutos de PC e Android: relógios monotônicos são locais. RTT não é latência visual.
- FPS enviado, decodificado, liberado à Surface e apresentado são métricas diferentes. Tela estática pode legitimamente enviar zero FPS.
- A capacidade de vídeo pertence ao decoder selecionado e a uma combinação de codec, perfil, tamanho, orientação e taxa. Um máximo isolado não resolve tudo.

## Implementação e escopo

- Faça mudanças pequenas, completas e revisáveis. Refatoração só quando necessária à tarefa; não misturar formatação global, atualização geral de dependências e mudança de comportamento.
- Não instalar drivers, parar serviços reais, alterar firewall, reiniciar computador, desinstalar APKs ou substituir credenciais só para testar uma hipótese. Primeiro use fakes, fixtures e ambiente de teste; siga a autorização vigente para efeitos no sistema.
- Não executar licenciamento real, compra, publicação, upload de arquivos privados ou release por inferência do backlog.
- Use falhas explícitas e mensagens úteis. Não transformar toda exceção em sucesso, nem usar retry sem limite.
- Mudanças de protocolo precisam de contrato escrito, fixtures dos dois lados e política de versão. O protocolo atual recusa v diferente de 3; campos novos opcionais não implicam compatibilidade automática.
- Nunca substituir uma biblioteca criptográfica por implementação própria. QR/código/fingerprint não são intercambiáveis sem análise do modelo de ameaça.
- Evite persistir estado efêmero, métricas por frame ou segredos em preferências exportáveis.
- Em FFI, revisar ownership, tamanho, alinhamento, stride, erros e vida útil de callbacks. Nada de panic cruzando fronteira C.
- Se faltar hardware, implemente e valide o que for possível, mas mantenha a validação física explicitamente pendente.

## Estados e conclusão

Estados: Planejada → Em andamento → Em validação → Concluída. Usar Bloqueada somente com obstáculo concreto, dependência e próximo passo registrados. Um spike usa a mesma sequência e entrega uma decisão.

Concluída exige:

- critérios de aceite da tarefa atendidos;
- testes pertinentes executados, comandos e resultados registrados;
- contratos/documentação atualizados;
- limites de plataforma e pendências explicitados;
- ausência de alterações fora do escopo;
- estratégia de reversão que preserve dados e compatibilidade.

Não marcar como concluída uma tarefa que exige teste físico ainda não realizado. Pode registrar “implementação pronta; validação física pendente”. Não é preciso rodar novamente todas as suítes por uma alteração textual sem impacto no código.

## Handoff para outra IA

Registre o que existe agora, arquivos alterados, decisões, resultados, pendências e o próximo passo exato. Não deixe instruções vagas como “otimizar tudo”. Um conflito entre o código atual e este documento deve resultar em atualização da evidência, não na implementação cega da descrição antiga.

## Prompt sugerido para selecionar uma tarefa

Substituir o ID e o nome pelo arquivo real listado no índice:

~~~text
Execute a tarefa TXX descrita em docs/tasks/TXX-nome.md.
Antes de alterar, leia docs/tasks/README.md, GUIA-EXECUCAO.md,
EVIDENCIAS.md, VALIDACAO.md e a tarefa. Confira as instruções locais,
o estado do Git e se as dependências já existem no código atual.
Revalide os achados; não implemente uma hipótese como se fosse fato.
Implemente o escopo da tarefa com testes pertinentes e compatibilidade
dos dois lados quando necessário. Registre decisões, resultados reais,
limitações e rollback usando TEMPLATE-ENTREGA.md.
Não execute outras tarefas opcionais nem publique/instale mudanças
no sistema fora do escopo autorizado. Se faltar uma dependência,
identifique a lacuna e avance apenas no trabalho independente seguro.
Atualize o status somente conforme a evidência disponível.
~~~

O prompt é um ponto de partida; instruções atuais do usuário prevalecem. Nenhum checklist elimina a necessidade de ler o código e avaliar o ambiente real.
