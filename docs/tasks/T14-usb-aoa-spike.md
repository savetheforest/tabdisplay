# T14 — Avaliar USB direto sem ADB e sem tethering

Prioridade: P3. Tipo: Spike opcional. Status: Concluída.
Dependências: T02, T05, T11, T12, T13.

## Pergunta e entrega

É possível oferecer um fluxo USB accessory/AOA confiável nos aparelhos-alvo, com custo de instalação aceitável? Entregar uma decisão técnica e, quando houver ambiente, um protótipo isolado. Não substituir os caminhos existentes nesta tarefa.

## Investigação

1. Ler as especificações AOA e UsbManager accessory. Confirmar suporte nos modelos reais; USB-C, USB host e USB accessory não são a mesma capacidade.
2. Mapear PC como host, Android em accessory mode, negociação, autorização, reenumeração e endpoints bulk.
3. No Windows, investigar driver/interface necessários e impacto em MTP/ADB. Não substituir driver USB genérico do usuário com ferramentas externas automaticamente.
4. No macOS, verificar libusb/API nativa, distribuição, assinatura e coexistência com outros aplicativos.
5. Medir throughput útil, RTT de aplicação, CPU e reconexão com USB 2/3, hubs, cabo de dados e cabo só de carga. USB-C não garante USB 3.
6. Projetar adaptador de transporte para o framing existente. Bulk USB continua um fluxo com leitura/escrita parcial; limites e cancelamento continuam necessários.
7. Manter autenticação e proteção criptográfica adequadas sobre o transporte. Presença de cabo não autoriza qualquer app/processo conectado.
8. Testar remoção durante frame, permissão negada, reenumeração, suspensão do PC e várias interfaces/aparelhos.
9. Verificar energia/carregamento e comportamento OEM; não prometer que todo tablet carregará mais rápido ou aceitará modo accessory.
10. Comparar AOA com ADB e tethering na mesma cena usando T05.

## Fora de escopo e restrições

- Sem driver de kernel próprio nesta fase.
- Sem root, APIs Android privadas ou alteração automática das opções de desenvolvedor.
- Sem habilitar AOA por padrão, modificar instalador ou remover ADB.
- “Viável na API” não é aprovação de produto nem resultado de benchmark.

## Critérios de aceite

- [ ] Matriz de aparelhos/sistemas testados e não testados.
- [ ] Desenho de permissões, autenticação, drivers e fallback.
- [ ] Medições comparáveis ou ausência de hardware explicitamente registrada.
- [ ] Decisão: seguir, restringir modelos ou não seguir; motivos e riscos concretos.
- [ ] Se seguir, criar tarefas menores de produção para host, Android, empacotamento e testes.

Fontes: [Android USB accessory](https://developer.android.com/develop/connectivity/usb/accessory) e [AOA 1.0](https://source.android.com/docs/core/interaction/accessories/aoa).

## Rollback

Encerrar o protótipo liberando interface/file descriptors e deixando o dispositivo no estado operacional documentado. Se o modo accessory tiver reenumerado o aparelho, testar o retorno ao uso normal por desconexão/reconexão controlada. Qualquer alteração de driver requer plano de recuperação separado; não restaurar drivers por tentativa e erro.
