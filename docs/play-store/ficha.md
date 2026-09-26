# Ficha da Google Play: o que preencher

Material pronto para colar no Play Console. Os passos no Console (criar o app, questionários, enviar para
revisão) precisam da conta de desenvolvedor e ficam com você.

## Antes de tudo
1. Gerar o `.aab` assinado: `cd android && ./gradlew bundleRelease` (com a keystore de release; ver README).
   Sai em `android/app/build/outputs/bundle/release/app-release.aab`.
2. Hospedar `docs/privacy.md` em uma URL estável (por exemplo GitHub Pages do repositório, se ele for público)
   e trocar `[SEU E-MAIL DE CONTATO]` no arquivo. A URL vai em **Política de privacidade**.
3. Play App Signing: aceitar; a keystore do README vira a *chave de upload*.

## Detalhes do app
| Campo | Valor |
|---|---|
| Nome (30) | TabDisplay |
| Descrição curta (80) | Use seu tablet Android como segundo monitor do PC ou do Mac. |
| Categoria | Ferramentas (ou Produtividade) |
| Tipo | Aplicativo, gratuito |
| Idioma padrão | Português (Brasil) |
| Ícone (512×512) | `docs/play-store/icon-512.png` |
| Gráfico de recursos (1024×500) | `docs/play-store/feature-graphic-1024x500.png` |
| Capturas de tela | `docs/play-store/screenshots/` (substitua por capturas do tablet de verdade, ideal: tablet 7" e 10", tela do PC estendida no tablet) |

### Descrição completa (até 4000)
```
Transforme o seu tablet Android em um segundo monitor para o PC (Windows) ou o Mac.

• Estender ou espelhar: use o tablet como uma tela a mais ou repita a tela do computador.
• Toque e caneta de verdade: gestos, rolagem com dois dedos e pressão da caneta funcionam no computador.
• Som do PC no tablet, com botão de silenciar só no tablet.
• Por cabo USB ou Wi‑Fi. Pelo cabo não precisa de código; pelo Wi‑Fi o pareamento é feito uma vez, com um código de 6 dígitos.
• Qualidade que se ajusta: Desempenho, Equilibrado, Qualidade ou Automático (baixa sozinho se a rede piorar). Dá para trocar direto pelo menu do tablet.
• Tudo local e criptografado (TLS): a imagem não passa por servidor nenhum e não coletamos dados.
• A sessão continua com a tela bloqueada ou com outro app na frente.

Para usar, instale também o TabDisplay no computador (Windows 10/11 ou macOS 14+), abra os dois e toque no nome do PC na lista.
```

## Segurança dos dados (Data safety)
- Coleta ou compartilha dados de usuário? **Não** (com a build sem Sentry).
  - Com a build que inclui Sentry: declarar **Registros de falhas** e **Diagnósticos**: coletados, opcionais para o funcionamento do app, tratados pelo Sentry como processador (não é compartilhamento) e não vinculados à identidade do usuário.
- Dados criptografados em trânsito: **Sim** (TLS 1.3).
- O usuário pode pedir a exclusão dos dados: não há dados no servidor; desinstalar apaga tudo do aparelho.

## Permissões e declarações
| Permissão | Motivo (para o formulário) |
|---|---|
| `INTERNET` | Conectar ao PC do próprio usuário (rede local ou USB). |
| `CHANGE_WIFI_MULTICAST_STATE` | Receber o aviso de presença do PC na rede local. |
| `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_DATA_SYNC` | Manter a sessão com o PC ativa (dados de tela/áudio chegando) com a tela bloqueada ou outro app aberto. Tipo de serviço: *dataSync*. |
| `WAKE_LOCK` | Impedir que a CPU durma durante uma sessão ativa. |
| `POST_NOTIFICATIONS` | Mostrar a notificação obrigatória do serviço em primeiro plano. |

Declaração de serviço em primeiro plano: descrever a mesma justificativa e, se pedirem vídeo, gravar uma
sessão com a tela do tablet bloqueada e desbloqueada.

## Classificação de conteúdo e público
- Questionário IARC: app utilitário, sem violência, sem conteúdo gerado por usuário, sem compras, sem
  localização: deve sair como **Livre**.
- Público-alvo: 18+ / adultos (não é direcionado a crianças).
- Anúncios: **não**. Compras no app: **não** (a licença do modo Estender vale só no app do PC).

## Requisitos técnicos
- `targetSdk` 36 / `compileSdk` 36 (atende ao requisito atual de API alvo; revisar a cada ano em agosto).
- `minSdk` 30 (Android 11+).
- Formato: `.aab` (App Bundle). Testado com o `bundletool` (build-apks + install-apks) em emulador x86_64.

## Checklist final
- [ ] `.aab` assinado gerado e enviado para uma faixa de teste interno.
- [ ] Política de privacidade no ar e URL preenchida.
- [ ] E-mail de contato trocado na política e na ficha.
- [ ] Capturas de tela reais do tablet (mín. 2; recomendado 4–8).
- [ ] Data safety, classificação de conteúdo, público-alvo e declaração de serviço em primeiro plano preenchidos.
- [ ] Teste interno com pelo menos uma instalação limpa (Play Store → conectar a um PC).
