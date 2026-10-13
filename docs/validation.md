# Validação — 12 e 13/10/2026

Os testes abaixo foram executados localmente no Windows, RTX 5060 de 8 GB, driver 610.88, Rust 1.99.0, MSVC 14.51.36231 e Windows SDK 10.0.26100. Monitores disponíveis: 1920×1080. A configuração de desenvolvimento anuncia 127.0.0.1 e mantém sinalização em loopback. Nenhum recurso remoto do homelab foi alterado.

| Verificação | Evidência |
|---|---|
| Compose principal e override | `config --quiet` passou |
| SFU customizado LiveKit 1.13.9 | Build Docker e teste Go de quota passaram; serviço HTTP local ativo |
| JWT real no RoomService | Válido aceito; adulterado recusado HTTP 401 |
| Emissor de tokens | Sete testes passaram, incluindo concorrência, expiração, sessões ativas e falha fechada |
| Quota no SFU | Cliente com JWT válido teve segundo screen_share e identidades hc-screen-4/unassigned recusados com LIMIT_EXCEEDED antes de publicação |
| Captura DXGI | Textura D3D11 em 1920×1080, sem readback CPU |
| AV1 local, amostra inicial 15 s | 119,57 frames codificados/s; aquisição p95 0,091 ms; cópia+encode p95 3,232 ms |
| HEVC local, amostra inicial 15 s | 119,05 frames codificados/s; aquisição p95 0,185 ms; cópia+encode p95 4,910 ms |
| Bitstreams locais AV1/HEVC | ffprobe confirmou 1920×1080, yuv420p e ausência de B-frames; ffmpeg decodificou |
| AV1 publicador → SFU → D3D11VA | 691 quadros, todos com textura D3D11, contador congelado após pausa |
| HEVC publicador → SFU → D3D11VA | 667 quadros; 29.099 pacotes RTP, 32.953.198 bytes e 2.148 quadros enviados no diagnóstico |
| Janela nativa e DirectX | Apresentou vídeo recebido por GPU; zero erro de apresentação nos smoke tests |
| Três publicadores AV1 | Grid → Destaque hc-screen-2 → pausa → Grid passou; primeira amostra: 3.704 quadros recebidos, 3.481 apresentações GPU |
| Minimização real da janela | Recepção ficou em 286 quadros tanto ao minimizar como ao restaurar; retomou em seguida |
| Desconectar/reentrar no mesmo processo | Registro de superfícies GPU voltou a zero antes de reconstruir o receptor; vídeo retomou sem erro |
| Publicação pela interface | Publicar → parar → publicar novamente criou duas faixas; prévia GPU BGRA apresentou sem erro |
| HEVC pela interface | Publicação, stop/republish, prévia GPU e espectador passaram; 717 quadros D3D11 recebidos |
| Rust sem GPU | Três testes passaram: política de visibilidade/miniaturas e transporte TLS do token |
| Instalador e ZIP | Inno Setup gerou ambos; instalação por usuário em diretório de teste, AV1 com 634 quadros GPU, desinstalação sem registro/executável residual |
| Layout | Inspecionado na janela real; print isolado do HWND, sem capturar janelas sobrepostas |
| 1440p@120 estáveis, latência fim a fim <100 ms | **Não medidos** |

Os números são amostras funcionais com conteúdo de desktop, algumas durante compilação e com vários processos no host. Não são benchmark controlado de conteúdo QHD a 120 Hz. O contador NVENC inclui repetição da última imagem da GPU: não representa 120 alterações distintas da tela. RTP/decode/apresentações contados não comprovam latência nem apresentação física a 120 Hz. Repaints podem apresentar um quadro mais de uma vez; esses contadores têm escopos diferentes.

## Correções descobertas durante a integração

- O modo single-PC produziu conflito ICE/SDP na combinação fixada. São usados dois transportes por cliente para o mesmo SFU.
- O libwebrtc pré-compilado usa CRT estático; Rust/ponte usam /MT consistente.
- FFmpeg interno do libwebrtc colidia com nomes avcodec_* do FFmpeg compartilhado. O decoder agora resolve explicitamente as DLLs fixadas ao lado do executável.
- HEVC do encoder externo precisava ser anunciado também em GetImplementations, além de GetSupportedFormats, e iniciar diretamente o passthrough.
- O SDK ocultava recusa AddTrack como timeout. O patch encaminha RequestResponse ao pedido correspondente.
- LiveKit NodeIP.UnmarshalString recusa outro IPv4 quando já preenchido; o CLI ignorava esse erro. O override monta livekit.dev.yaml sem node_ip, deixando a opção CLI preencher o endereço de desenvolvimento.
- Logs verbosos do publicador não podem ficar num pipe sem leitor durante o teste. A integração registra diagnóstico em arquivo ignorado, evitando bloquear as threads WebRTC.

## Repetir

```powershell
./tools/cargo-native.ps1 test --locked --features desktop
python -m unittest discover -s tests -v
python -m tests.integration_tokens
python -m tests.integration_quota
python -m tests.integration_media
$env:HYPERCAST_TEST_CODEC = 'h265'
python -m tests.integration_media
Remove-Item Env:HYPERCAST_TEST_CODEC
python -m tests.integration_desktop
python -m tests.integration_multishare
python -m tests.integration_gui_publish
./tools/package.ps1
./tools/test-installer.ps1
```

Integrações de mídia exigem SFU local e hardware compatível. O teste visual usa Pillow em .tools/python-libs. Screenshots de salas reais ficam em .tools e não são versionados; assets/layout.png contém apenas a tela inicial do próprio aplicativo, sem token/mídia de outros aplicativos.

Ainda faltam medições QHD/120 com movimento, jitter/perda e rede real; AMF, HDR, composição de cursor separado, miniaturas vivas e recuperação automática completa de sessão não fazem parte desta versão. As miniaturas atuais são quadros pausados, explicitamente identificados na interface.
