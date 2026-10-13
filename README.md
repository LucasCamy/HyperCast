# HyperCast

![Interface nativa HyperCast](assets/layout.png)

Aplicativo nativo Windows para compartilhar telas por SFU LiveKit centralizado. Pipeline DXGI → NVENC → WebRTC → D3D11VA → swapchain DirectX, interface egui e até três publicadores por sala. Imagens brutas ficam na GPU; cópias entre texturas GPU e bitstream comprimido em RAM fazem parte do pipeline.

AV1 e HEVC, apresentação GPU, três publicadores e Grid/Destaque foram exercitados em 1920×1080. **1440p@120 estáveis e latência fim a fim <100 ms são metas ainda não comprovadas.** Veja [validação](docs/validation.md).

## Usar o aplicativo

Abra `hypercast.exe` do ZIP ou instale a versão por usuário. Os pacotes incluem bibliotecas de mídia: o usuário não precisa instalar Rust, Python, FFmpeg ou Visual C++ Redistributable. Precisam de Windows 10 2004+ x64 e driver gráfico compatível. Publicação usa NVIDIA NVENC, driver 610+; AMF não está implementado.

Informe servidor e token, entre, escolha o monitor e use **Compartilhar minha tela**. Token de espectador habilita apenas recepção. URL/token não são persistidos pela interface. A API secret fica no emissor administrativo.

**Grid** recebe até três telas. **Destaque** recebe a escolhida e cancela as outras subscrições. Miniaturas guardam o último quadro visto: são estáticas, sem tráfego ou decode contínuo. Clique no participante/nome da miniatura para trocar o foco. **Pausar recepção** cancela todas as subscrições. Minimizar também pausa; restaurar reaplica a seleção anterior. O SFU não transcodifica nem produz versões reduzidas.

## Servidor e tokens

```powershell
python tools/init_env.py
docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d --build
python tools/room_token.py --room amigos --name Lucas --role publisher
python tools/room_token.py --room amigos --name Amigo --role viewer
```

O emissor administrativo imprime JWTs bearer de cinco minutos. Distribua-os por canal seguro. `init_env.py` cria chaves aleatórias sem sobrescrever `.env`. Nenhuma chave de servidor deve entrar no instalador.

Compose principal: rede host para Linux. Override: Docker Desktop, sinalização em loopback. Cliente exige WSS fora de loopback. Para produção, ajuste o IP anunciado, configure HTTPS/WSS e restrinja RoomService. Não há TURN; redes que bloqueiam UDP/ICE-TCP podem falhar. O homelab foi analisado, sem alterar recursos remotos: [topologia](docs/homelab.md).

| Porta | Transporte | Função |
|---|---|---|
| 8100 | TCP | Sinalização e RoomService |
| 8101 | TCP | ICE/TCP |
| 8102–8105 | UDP | RTP/RTCP mux |

`infra/90-hypercast.conf`: ajustes opcionais de socket no host Linux. O buffer NACK de 1024 pacotes é histórico de retransmissão, não playout delay. Encoder permite 35–45 Mbps, sujeito a adaptação por congestionamento.

## Quota de compartilhamentos

`HYPERCAST_MAX_PUBLISHERS` aceita 1–3. SQLite `BEGIN IMMEDIATE` reserva identidades `hc-screen-1..3` por sala; consulta ao SFU mantém sessões conectadas ocupando vagas mesmo após expiração do token. A quarta solicitação é recusada. Todos os emissores devem usar esse mesmo banco; não apague reservas para contornar o limite.

A imagem **hypercast-sfu:1.13.9-1** aplica admissão síncrona ao LiveKit fixado. Cada vaga publica uma única faixa `screen_share`; faixas pendentes e ativas contam. Segunda tela na mesma vaga, identidades fora do pool e outras fontes são rejeitadas antes da publicação. O SDK recebe `LIMIT_EXCEEDED` imediatamente. Trocar a imagem pelo LiveKit oficial remove essa proteção. Reduzir a quota exige drenar sessões/reservas incompatíveis. Limites YAML do nó não substituem essa quota. Sala permite doze participantes e três subscrições de vídeo.

## Desenvolvimento e instalador

Rust 1.92+ (testado 1.99), Visual Studio C++/Windows SDK e Git. O helper configura MSVC/CRT estático e baixa FFmpeg LGPL fixado por SHA256. DLLs são resolvidas ao lado do executável para evitar colisão com o FFmpeg incorporado no libwebrtc.

```powershell
./tools/cargo-native.ps1 build --locked --features desktop
./target/x86_64-pc-windows-msvc/debug/hypercast.exe --gui
./tools/cargo-native.ps1 test --locked --features desktop
winget install --id JRSoftware.InnoSetup --exact
./tools/package.ps1
```

Saídas em `dist/`: instalador por usuário, ZIP portátil e SHA256SUMS. Build sem assinatura Authenticode. [Licenças](docs/THIRD_PARTY.md), [manual portátil](docs/portable.md) e [patches do SDK](vendor/README.md).

## Diagnósticos e testes

O CLI mantém captura/encode/publicação e recepção sem janela. Use `HYPERCAST_ROOM_TOKEN` no ambiente, nunca na linha de comando. Publicação: `--encode --codec av1 --fps 120 --seconds 30 --publish-url ws://127.0.0.1:8100`. Espectador: `--receive-url ws://127.0.0.1:8100 --view-mode grid --seconds 30`. Destaque aceita `--focus hc-screen-1`; `--pause-after 6` testa pausa. `--list` enumera monitores. `--dump` grava bitstream somente quando solicitado explicitamente.

```powershell
python -m unittest discover -s tests -v
python -m tests.integration_tokens
python -m tests.integration_quota
python -m tests.integration_media
python -m tests.integration_desktop
python -m tests.integration_multishare
python -m tests.integration_gui_publish
```

Integrações de mídia exigem GPU e SFU local. Screenshots de QA ficam apenas em `.tools`, ignorado pelo Git; o teste visual exige Pillow no ambiente de desenvolvimento. CI compila/testa sem GPU e valida autenticação e build/testes do SFU.

## Limites da versão

Interface publica AV1 (padrão) ou HEVC, selecionável antes de compartilhar, camada única, P1 ultra-low-latency, CBR, sem B-frames/lookahead e GOP de um segundo. DXGI é não bloqueante; timer de alta resolução descarta ticks perdidos. Desktop estático repete a última imagem: 120 frames codificados/s não significam 120 mudanças distintas de conteúdo.

Não há AMF, composição de cursor separado, HDR, miniaturas vivas em camada reduzida ou recuperação automática completa após falha de rede/dispositivo. `ACCESS_LOST` tenta reconstruir DXGI no mesmo dispositivo. Há dois peer connections **para o mesmo SFU** por conflito ICE observado no modo single-PC da combinação fixada. Latência, jitter/perda, alta movimentação e 1440p@120 precisam de medição antes de uso em produção.
