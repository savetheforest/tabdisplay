# TabDisplay no Linux: análise de viabilidade

Data: 26/09/2026. Resumo: **é viável**, mas por fases. O app do tablet e o protocolo não mudam. No PC, o que muda
é a camada de plataforma (`desktop/src-tauri/src/{win,mac}/`), e o modo **Espelhar** é bem mais barato que o **Estender**,
que depende do compositor (X11, GNOME, KDE, wlroots).

## Onde o código está hoje
O resto do app só fala com um módulo `sys::…` que cada sistema implementa com os mesmos nomes:

| Módulo | O que o servidor usa | Windows | Mac |
|---|---|---|---|
| `capture` | `Capture::open(device, fit)`, `next()`, `monitors()` | Desktop Duplication | ScreenCaptureKit |
| `display` | `attach()`, `VirtualDisplay::configure`, `find_devices()`, `PRESETS`, `driver_state()` | Virtual Display Driver + serviço | `CGVirtualDisplay` |
| `encode` | `HwEncoder::new/encode` (o servidor cai no openh264 se falhar) | Media Foundation | VideoToolbox |
| `input` | `Injector::inject/scroll` | toque/caneta sintéticos | eventos CGEvent |
| `audio` | `Loopback::open/read` | WASAPI loopback | ScreenCaptureKit |

Portar = criar `linux/` com essa mesma superfície (hoje `lib.rs` só compila com `cfg(windows)` ou `cfg(target_os = "macos")`).
Não há Windows-específico fora de `win/` além do serviço/driver e de trechos `cfg(windows)` no `lib.rs` (instalar driver, autostart).
Opus, H.264 por CPU (openh264), TLS, pareamento, licença, updater, Sentry e a UI (Tauri/WebKitGTK) já são multiplataforma.

## Peça por peça

| Peça | Como fazer no Linux | Dificuldade |
|---|---|---|
| **Captura** | Wayland: portal `org.freedesktop.portal.ScreenCast` + PipeWire (crates `ashpd` + `pipewire`); o usuário aprova uma vez (dá para guardar o token). X11: `XShm`/XComposite (ex.: crate `xcap`/`scap`). Os dois cobrem GNOME, KDE e wlroots. | Média |
| **Monitor virtual (Estender)** | Não existe API única. **X11:** `xrandr --setmonitor` (recorte de uma tela existente) ou saída dummy/`VirtualHeads` (depende da GPU: Intel ok, NVIDIA exige EDID/dummy). **GNOME (Mutter):** `org.gnome.Mutter.ScreenCast.RecordVirtual` cria um monitor virtual de verdade. **KDE:** virtual output do KWin (`zkde_screencast`). **Sway/Hyprland:** `create_output` headless. Alternativa universal, porém pesada: módulo de kernel `evdi` (DKMS + root). | **Alta**: uma implementação por compositor |
| **Codificação** | Começar com openh264 (CPU), que já é o fallback do servidor. Depois VAAPI (Intel/AMD) e NVENC via GStreamer (`pipewiresrc ! … ! vaapih264enc`) ou `libva` direto. | Baixa (CPU) / Média (GPU) |
| **Entrada (toque e caneta)** | `uinput`: dispositivo virtual de toque multitouch e de caneta com pressão e inclinação. O Linux reconhece de verdade (gestos, Ink-like nos apps), como no Windows. Precisa de acesso a `/dev/uinput` (regra udev ou grupo `input`). Sem root: portal RemoteDesktop/`libei` (pede aprovação). | Média |
| **Áudio** | Fonte "monitor" do PulseAudio/PipeWire (crate `libpulse-binding` ou `cpal`), entra direto no Opus que já existe. | Baixa |
| **Empacotamento** | Tauri gera AppImage, `.deb` e `.rpm`. Dependências: WebKitGTK 4.1, `libayatana-appindicator3` (bandeja). O `adb` do Android sai do pacote da distro ou do zip de platform-tools para Linux. O updater do Tauri atualiza AppImage. | Baixa |
| **Permissões/UX** | Guia de primeiro uso: portal (captura/entrada), regra udev do uinput, `adb` e regra udev de USB para o tablet (`android-udev-rules`). Pareamento, TLS e licença ficam iguais. | Baixa |

## Fases sugeridas
1. **Espelhar, Ubuntu/GNOME e KDE (Wayland + X11)**: captura por portal/X11, openh264, áudio, uinput, AppImage/`.deb`, CI em Ubuntu.
   Entrega o "tablet como segundo monitor espelhado com toque/caneta". Esforço estimado: **2 a 3 semanas** para uma pessoa.
2. **Estender no GNOME (Mutter `RecordVirtual`)** e **no X11 (xrandr/dummy)**: cobre a maior parte dos usuários. **~2 semanas** cada.
3. KDE e wlroots para Estender, e codificação por GPU (VAAPI/NVENC): **1 a 2 semanas** por item.

## Riscos e pontos de atenção
- **Fragmentação:** Estender muda por compositor e por versão; cada um precisa de testes em máquina real. Sugestão: declarar suporte só ao que for testado (por exemplo Ubuntu 24.04/GNOME 46 e Fedora/KDE).
- **NVIDIA proprietária:** captura por PipeWire funciona, mas o monitor virtual em X11 é o caso mais chato.
- **Wayland e aprovações:** captura e entrada pedem confirmação do usuário (portal); é uma etapa a mais no primeiro uso.
- **Testabilidade:** nada disso roda aqui no Windows nem no Mac de teste. Precisa de uma máquina/VM Linux (Ubuntu com GNOME e outra com KDE) e de um tablet no USB (a VM precisa de repasse de USB para o `adb`).
- **Licença do driver:** Estender via `evdi` traria dependência GPL de kernel; preferir as APIs dos compositores.
- **Mercado:** o segmento é pequeno e os concorrentes quase não cobrem Linux; vale confirmar demanda antes das fases 2 e 3.

## Recomendação
Fazer a **fase 1** (custo baixo, reaproveita quase tudo) e lançar como "Espelhar no Linux (beta)". Só investir em Estender depois de
medir interesse, começando pelo GNOME/Wayland. Passo prático para começar: adicionar `linux/` em `lib.rs`, um job Ubuntu no
`build.yml` (que gere AppImage e `.deb`) e uma VM Ubuntu para testar captura e `uinput`.
