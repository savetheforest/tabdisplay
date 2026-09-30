# T14 — USB direto via AOA: viabilidade

## Identificação

- ID e título: T14 — Avaliar USB direto sem ADB e sem tethering
- Status: Concluída — decisão: não seguir nesta versão
- Base: `d823e5b` + T01–T13 parciais + working tree de 29/09/2026

## Resultado e decisão

O AOA é tecnicamente compatível com um adaptador de transporte baseado em fluxo,
mas não há evidência suficiente para adotá-lo no produto atual. A decisão é não
implementar AOA nesta versão e manter ADB/reverse, IPv4/manual e TLS como os
caminhos suportados.

A documentação oficial informa que o Android em modo accessory depende de um
host/accessory USB, exige autorização do usuário e não tem suporte garantido em
todo aparelho ou fabricante. O fato de o tablet ter USB-C, USB host ou ADB não
prova que ele aceite o modo accessory. O AOA também limita a coexistência a um
accessory por vez, o que aumenta o risco operacional durante reenumeração,
carregamento e uso de MTP/ADB.

## Matriz de ambiente

| Ambiente | Estado | Evidência/limitação |
|---|---|---|
| Redmi Pad 2 `25040RP0AE`, Android API 36 | Detectado por ADB USB | AOA não exercitado; atualização/abertura do APK no aparelho não foi autorizada |
| Windows host | Não testado | Seria necessário validar interface, driver e coexistência com MTP/ADB sem alterar drivers automaticamente |
| macOS host | Não testado | Seria necessário validar API/libusb, assinatura, distribuição e coexistência com outros aplicativos |
| Cabo USB 2/3, hub, cabo só de carga | Não medido | Não há throughput, RTT, CPU ou reconexão comparáveis |

Consequentemente, os critérios de benchmark, remoção durante frame,
reenumeração, suspensão e permissão negada permanecem sem medição. Nenhum
resultado de compatibilidade ou desempenho foi inferido a partir da presença do
conector USB-C.

## Desenho se retomado

O PC seria o host/accessory e o Android usaria `UsbManager` para descobrir,
solicitar permissão e abrir os endpoints bulk. O adaptador entregaria leituras e
escritas parciais ao framing v3 existente; limites, cancelamento e timeouts
continuariam obrigatórios. TLS, token e revogação de T11 permaneceriam acima do
transporte: presença física do cabo não autorizaria qualquer processo.

Uma retomada exigiria tarefas separadas para host Windows, host macOS, cliente
Android, empacotamento e matriz de testes. Drivers de kernel, root, APIs
privadas, alteração automática de opções de desenvolvedor e remoção de ADB
ficam fora do escopo.

## Riscos e fallback

- Suporte AOA e comportamento de energia variam por aparelho/OEM.
- Reenumeração pode interromper o accessory, MTP ou ADB; um cabo USB-C não
  garante USB 3 nem mesmo modo accessory.
- A ausência de driver dedicado não elimina diferenças entre Windows e macOS.
- Sem laboratório autorizado não é possível comparar AOA com ADB/tethering sob a
  mesma cena e as métricas de T05.

O fallback atual permanece operacional: ADB selecionado por serial, endpoint
local explicitamente rotulado como ADB/local, descoberta IPv4/manual e
autenticação TLS/token. Não houve mudança de driver, instalação no tablet,
alteração de dados ou habilitação de AOA.

## Fontes

- [Android USB accessory](https://developer.android.com/develop/connectivity/usb/accessory)
- [Android Open Accessory protocol](https://source.android.com/docs/core/interaction/accessories/aoa)

## Reversão

Não houve protótipo nem alteração a reverter. Uma futura implementação deverá
fechar streams/file descriptors, tratar permissão e reenumeração e confirmar o
retorno ao uso normal por desconexão/reconexão controlada.
