# T24 — Entregar builds, instaladores e atualizações recuperáveis

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T11, T19, T23, T34.

## Contexto

Builds podem usar chave debug quando a release key falta; platform-tools é baixado como latest; hooks NSIS esperam 1,5 s e prosseguem após comandos de serviço. APK, desktop e protocolo precisam permanecer compatíveis. Mac embute apenas APK no config atual.

## Implementação

1. Distinguir build de desenvolvimento de release distribuível. Release de produção deve falhar claramente se assinatura exigida não estiver disponível; artefatos de teste recebem identificação explícita.
2. Verificar versão coerente em Cargo.toml, package.json, tauri.conf.json, versionName/versionCode e matriz de protocolo. Não aumentar versões sem escopo de release.
3. Fixar versões e verificar integridade dos binários baixados de ADB/driver; registrar origem, hash e notices/licenças. Não remover o pin de hash do driver existente.
4. Definir ADB Mac por arquitetura, permissões executáveis e assinatura, ou documentar pré-requisito honesto; validar instalação limpa sem PATH de desenvolvedor.
5. No NSIS, esperar estado de serviço com prazo e verificar códigos de retorno. Falhas de instalação de driver/firewall/serviço não podem ser apresentadas como sucesso.
6. Revisar regra de firewall com escopo mínimo suficiente e UX para rede pública; não desabilitar firewall global nem abrir roteador.
7. Preservar driver preexistente, XML e dados de usuário em upgrade/uninstall. Registrar ownership do que o TabDisplay instalou.
8. Testar updater com manifesto/pacote adulterado, assinatura errada, interrupção de download, disco cheio e incompatibilidade de versão.
9. Coordenar atualização Android/desktop: informar incompatibilidade, oferecer caminho de atualização e preservar instalação anterior. Nunca desinstalar Android silenciosamente por assinatura divergente.
10. Distinguir chave de upload Play e assinatura do APK distribuído via USB. Planejar continuidade entre canais; não assumir que APK assinado pela upload key atualiza app assinado pela Play App Signing key.
11. Verificar assinatura/notarização das saídas e compatibilidade de permissões macOS; não prometer ausência de alertas SmartScreen/Gatekeeper sem verificação.
12. Produzir instruções de rollback e recuperação para atualização parcial, sem publicar release nesta etapa.

## Critérios de aceite

- [ ] Artefato de produção não é silenciosamente assinado com debug.
- [ ] Instalação/upgrade têm resultado verificável por componente.
- [ ] Atualização adulterada é recusada; teste existente continua passando.
- [ ] Instalação limpa e upgrade preservam preferências/pareamento/licença.
- [ ] Desinstalação não remove driver/arquivos que pertenciam a outro software.
- [ ] Canais Play/USB têm estratégia de certificado documentada.

## Validação

VMs/sistemas designados: instalar, atualizar, reparar, cancelar e desinstalar; manter snapshots e comparar serviços/regras/driver antes/depois. Em Mac real, verificar pacote e permissões.

Não modificar segredos, gerar nova chave de produção, enviar à loja ou publicar GitHub release só porque o checklist existe. Essas ações seguem o pedido de execução vigente.
