# T13 — Mostrar e selecionar corretamente caminhos locais, inclusive USB

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01, T11, T12. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Contexto

Discovery identifica USB apenas por 127.0.0.1 e lista o resto como Wi-Fi. Um PC pode aparecer por vários IPs. status usa UDP connect a 8.8.8.8 para obter IP de uma rota; essa chamada não envia o vídeo à internet, mas pode escolher VPN/interface errada.

## Implementação

1. Modelar PC/identidade separadamente de endpoints e transporte observado. Agrupar mesmo pc_id anunciado sem tratá-lo como autenticado até T11.
2. Enumerar interfaces locais no PC e apresentar endereços úteis por tipo conhecido. Não selecionar primeira interface indiscriminadamente; considerar VPN, Ethernet, virtual e rede USB.
3. Remover dependência de um destino público para decidir IP exibido; manter a permissão INTERNET para sockets Android.
4. Classificar somente com evidência. Quando não for possível provar “USB tethering”, mostrar “Rede local” e endereço, não inferir por faixa 192.168.x.
5. Implementar preferência USB conhecida, depois último endpoint válido, sem migrar sessão ativa ou trocar identidade automaticamente.
6. Definir probe autenticado/limitado e seleção de rota Android quando houver múltiplas redes. Rede sem validação de internet pode servir o PC e não deve ser descartada por isso.
7. Tratar criação/bind de DatagramSocket falhando, conflitos de porta, start/stop repetidos, lock multicast e expiração por relógio monotônico.
8. Manter conexão manual com validação de host/porta suportados e mensagens para DNS/rota/firewall. No contrato atual a porta é fixa; não aceitar sintaxe que a implementação não entende.
9. Avaliar IPv6 e mDNS como etapas delimitadas: adicionar somente com listeners, scope-id e testes completos; caso contrário documentar IPv4 como suporte atual.
10. Explicar tethering como “rede local pelo cabo; não exige internet para transmitir”. Fabricante/operadora podem limitar habilitação; oferecer fallback sem prometer ativação programática.
11. Separar acesso externo opcional de atualização/telemetria da sessão, e exibir a conexão realmente usada.

## Critérios de aceite

- [ ] PC com Wi-Fi/Ethernet/VPN não é sempre apresentado com IP da rota errada.
- [ ] Mesmo PC pode ter vários endpoints sem cartões indistinguíveis.
- [ ] ADB e rede USB continuam operando sem acesso à internet em ambiente de teste.
- [ ] Não há requisito de internet validada para aceitar rede local utilizável.
- [ ] Falha de descoberta não crasha a Activity; conexão manual continua disponível.

## Testes

Fixtures de interfaces, expiração e beacons inválidos; dois PCs com mesmo nome; mesmo pc_id com certificado divergente; alteração de IP; Wi-Fi e USB simultâneos; AP que bloqueia broadcast; cabo apenas de carga.

Teste offline em ambiente controlado mantendo as interfaces necessárias: não desabilitar a rede da máquina do usuário sem autorização. Registrar tráfego externo do updater/Sentry separadamente.

Rollback mantém descoberta IPv4/manual e autenticação; não reintroduzir requisito de internet.
