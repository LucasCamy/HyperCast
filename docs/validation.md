# Validação — 12 e 13/10/2026

## Controle de volume — 0.3.2, 13/10/2026

A rail do slider herdava o mesmo `inactive.bg_fill` da lateral e desaparecia
no fundo. O estilo agora é aplicado somente ao controle de volume: rail
contrastante, preenchimento verde e marcador circular. Porcentagem em label
separado substitui o campo numérico; label de acessibilidade é associado ao
slider nativo. `send_layout` e a semântica de silenciamento permanecem iguais.
Build desktop passou e a janela conectada real foi inspecionada visualmente
com 80%. Não foram repetidas integrações de rede para esta alteração visual.

## Layout desktop — 0.3.1, 13/10/2026

UI/UX Pro Max aplicada ao cliente egui; padrões de seleção de tela e controles
de sessão de Discord/Zoom consultados. Não foram adicionados frameworks web,
gravação/miniaturas da área de trabalho ou animações contínuas.

Seleção de monitor/resolução e HEVC/AV1 agora fica acima do canvas; controles
de publicação, parada, pausa, áudio e saída ficam na barra inferior. A lateral
usa cartões de participantes com iniciais, nome, estado e seleção de foco.
Formulário desaparece após conectar; servidor/token ficam em acesso avançado.
Monitor/formato ficam bloqueados durante publicação para preservar o pipeline.

Janela real inspecionada em 1320×820 e 960×620; sessão real isolada com três
nomes sintéticos e publicações sem pixels confirmou os cartões. Screenshot
versionado mostra somente a janela inicial, sem convite/token/mídia.

ZIP final passou na integração de áudio/vídeo via TURN TLS: 1212 frames enviados
no último ciclo, 1485 recebidos e 1451 apresentações GPU; stop/republication e
silenciamento/retomada de áudio passaram. Uma tentativa inicial teve timeout
de ICE; a repetição conectou. Outro resultado de teste confundiu logs de duas
GUIs de revisão com os do teste de mídia; o teste passou a selecionar por PID.
Não é evidência de latência <100 ms ou estabilidade QHD120 sob movimento.
Instalador gerado; instalação/desinstalação 0.3.1 não repetida.

## Áudio do sistema — 0.3.0, 13/10/2026

Servidor SFU e API implantados no k3s após o proprietário sair da sala e
autorizar o reinício. Mesmas portas/DNS/TURN; Opus habilitado, subscrição de
áudio limitada a três. Novos JWTs de publicador permitem tela e áudio de tela.
Testes Go incluem áudio pareado, duplicata, áudio isolado, microfone e quota
concorrente. Dezesseis testes Python e seis Rust passaram. Integração de quota
contra o SFU atualizado manteve três telas, quarta recusada e liberação ao parar.

`tests/system-audio.ps1` passou: a captura excluiu o próprio processo enquanto
ele reproduzia um tom sintético; limites de buffers PCM foram respeitados.
Uma primeira tentativa coincidiu com FFplay externo de outro teste; repetir
sem esse áudio externo confirmou a exclusão. Não foram gravados sons.

`tests.integration_system_audio` passou com o executável do ZIP final 0.3.0:
som gerado externamente → captura PCM → Opus → SFU/TURN TLS → decode de
áudio não silencioso com canais L/R distintos → WASAPI playout. O receptor
renderizou zeros no teste para evitar feedback entre dois processos no mesmo
PC. Detectar os canais separados exigiu sinalizar estéreo no AddTrack e no
SDP answer do subscriber; estéreo permaneceu correto após parar/republicar.
As janelas de silenciamento da GUI foram associadas ao timestamp de volume
zero nos logs, confirmando zero sinks e retomada do contador após reativar.

Layout revisado na janela do aplicativo, sem credenciais/mídia: checkbox
**Áudio do sistema** visível na barra de compartilhamento; volume e mute na
lateral. ZIP e instalador gerados; instalação/desinstalação 0.3.0 não repetida.
Não comprova sincronismo físico áudio/vídeo, latência <100 ms ou QHD120 em
movimento. Som do sistema requer Windows build 20348+; hotplug pode exigir
desativar/reativar. Mais detalhes em [áudio do sistema](system-audio.md).

## Correções de captura HDR e encerramento — 0.2.1, 13/10/2026

O diagnóstico `message (6).txt` da versão 0.2.0 mostra textura mudando para
`DXGI_FORMAT_R16G16B16A16_FLOAT` (10), além de recusas por publicação duplicada
da mesma sessão. Não são evidência de excesso de participantes nem de falha TLS.
O cliente agora converte scRGB linear para SDR/sRGB na GPU com compressão de
highlights, mantendo BGRA8 sem conversão. Não preserva sinalização/HDR original.

`tests/capture-color.ps1` passou na GPU local: preto, meio-tom sRGB, compressão
monotônica de highlights, opacidade e reutilização do pass. O mesmo teste passou
pela ponte NVENC real com texturas sintéticas HDR→SDR→HDR e repetição sem novo
frame. Somente o teste faz readback de pixels sintéticos para conferir a cor.
Ainda falta confirmar aparência e estabilidade no monitor HDR do usuário.

Unpublish agora encerra o transceiver RTP antes de negociar; erros de parada
são registrados e duplicação tem mensagem distinta da quota de três telas.
`python -m tests.integration_publication_lifecycle` passou contra o SFU existente:
quatro publicações reais HEVC na mesma conexão, sem gravar vídeo. Consultas
RoomService confirmaram que nas três janelas de parada havia um participante
conectado e zero tracks. Nenhuma atualização do servidor foi necessária.

A integração também passou com o executável do ZIP final 0.2.1: quatro
publicações, três remoções confirmadas no SFU e 499 frames enviados no último
ciclo. Seis testes Rust passaram. ZIP e instalador foram gerados com SHA-256
em `dist/SHA256SUMS.txt`; instalação/desinstalação 0.2.1 não foi repetida.

Os testes não comprovam latência fim a fim abaixo de 100 ms nem QHD/120 em HDR.

## Convites, participação livre e TURN/TLS — 13/10/2026

Coturn oficial 4.18.0 por digest, Caddy por digest e backend de convites foram
implantados no k3s. SFU atualizado por digest para quota atômica por sala e
participante, aplicada antes do ACK AddTrack. Até doze membros podem entrar
com canPublish=true; somente três telas pendentes/ativas são admitidas. Segunda
tela do mesmo membro continua recusada. Parar uma tela libera vaga sem sair.

Testes Go de identidade/fonte/duplicação e corrida de doze reservas para três
vagas passaram no build. Teste nativo contra produção, sem capturar pixels,
confirmou três publicações pendentes admitidas, quarto membro conectado e quarta
publicação recusada; depois de unpublish, esse membro publicou enquanto o primeiro
continuava conectado. Isso cobre cancelamento/liberação de publicação pendente.

TURN WAN TCP 5349 passou TLS com certificado validado. Teste HEVC publisher→SFU→
receiver com ambos relay-only recebeu 637 quadros D3D11 e respeitou pausa.
Stats indicaram local relay=true e relay transport=Some(Tls) nos dois clientes.
Candidato remoto privado é o peer local SFU; entrada WAN foi por Ethernet e
gateway 192.168.30.1. RTT de 2 ms não comprova latência fim a fim. Hook de
renovação sincronizou o Secret TLS e reiniciou coturn com sucesso.

API guarda hash de convite, validade e máximo de dispositivos. Identidade e
grants vêm do servidor; o cliente guarda somente ID aleatório para reentrada
idempotente. Dezesseis testes Python e seis Rust passaram. GUI trocou convite
por JWT e publicou/parou/republicou com relay-only. Revogação bloqueia novas
emissões, sem expulsar usuários já conectados. Gateway público preserva WSS
e RoomService no hostname anterior; API/segredos não foram expostos no roteador.

AMF continua não implementado: proprietário confirmou ausência de GPU AMD
disponível com monitor ativo para validação. Miniaturas vivas e medição fim a fim
QHD/120 ainda pendentes. Não confundir essas entregas com comprovação sub-100 ms.

## Diagnóstico automático e recuperação — 13/10/2026

Interface passou a persistir logs limitados a 1 MiB, com retenção de até 16
arquivos GUI e exportação para clipboard. JWT atual e padrões JWT são redigidos;
não há gravação de vídeo. Falha de escrita não impede entrada na sala.
HEVC/H.265 é agora o padrão da interface; AV1 permanece selecionável.

Encoder e publicação usam as dimensões da superfície de duplicação DXGI, não
as coordenadas do desktop. Erros nativos diferenciam device, dimensões e formato.
O worker reconstrói captura/encoder/publicação após erros recuperáveis, com
intervalo de um segundo e até três tentativas por ciclo de compartilhamento.
O contador `sent` continua sendo do encoder/publicação atual, e pode reiniciar
ao republicar. Formatos incompatíveis não entram em repetição automática.

Seis testes Rust passaram, incluindo redação de token e classificação de erros.
Publicar/parar/republicar e espectador passaram com 1.155 quadros D3D11.
Um teste adicional injetou perda de captura somente no build debug: houve uma
reconstrução e republicação bem-sucedidas, sem token/API secret no arquivo GUI.
Esse teste exercita o caminho de recuperação com NVENC e SFU reais; não simula
todos os efeitos de remoção física da GPU, troca real de modo ou bloqueio do Windows.
Validação dessas condições no PC do colega continua necessária.
Doze testes Python passaram. Pacote release HEVC entrou pelo WSS público,
publicou/parou/republicou duas faixas e terminou conectado sem erro. O caminho
ICE do publicador foi UDP não privado; o subscriber vazio escolheu candidato
privado. Isso não é medição fim a fim nem teste relay-only.

TURN/TLS recebeu inspeção e guia em turn-tls.md; não foi instalado.
Convites/emissão automática permanecem na próxima etapa.

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

Ainda faltam medições QHD/120 com movimento, jitter/perda e rede real; AMF, transmissão preservando HDR, composição de cursor separado, miniaturas vivas e recuperação automática completa de sessão não fazem parte desta versão. A captura scRGB/FP16 é convertida para SDR na GPU. As miniaturas atuais são quadros pausados, explicitamente identificados na interface.

## Implantação no homelab — 13/10/2026

SFU instalado no nó vm-200 do k3s, namespace hypercast: pod 1/1 Running, HTTP 200, imagem por digest, Secret exclusivo e tetos UDP de 16 MiB. Manifests passaram dry-run na API real do cluster. Quota remota rejeitou duplicação e identidades fora do pool. AV1 recebeu 650 quadros D3D11 por LAN/VPN; WSS via hypercast.cloudlane.com.br recebeu 591 quadros D3D11, também com mídia pela VPN. HTTPS/WSS público não prova alcance das portas de mídia pela internet.

O teste público revelou ausência do feature TLS no cliente: habilitado native-tls do SDK (Schannel no Windows), recompilado e testado com certificado Cloudflare. Cloudflare recusou o User-Agent padrão Python-urllib com erro 1010; o emissor passou a identificar-se honestamente como HyperCast-Operator, mantendo assinatura, HTTPS e proteção contra redirecionamentos. Nenhuma regra de segurança Cloudflare foi desabilitada.

Doze testes Python passaram, incluindo renderização Kubernetes e seleção explícita de ambiente do operador. Quatro testes Rust passaram após incluir classificação de rota ICE. Diagnóstico agora mostra protocolo, classe da rota remota e RTT de rede sem imprimir endereços/credenciais ICE; RTT não é latência fim a fim. Instalador TLS passou instalação, GPU e remoção; ZIP TLS passou WSS contra o SFU remoto. O pacote inclui Test-Streaming.ps1 para o PC QHD, com entrada oculta de JWT e logs sem gravação de vídeo.

O proprietário informou outro PC com RTX 4080, driver 32.0.16.1714 e monitor 2560×1440 a 239 Hz. O primeiro teste QHD nesse PC está registrado abaixo. A Radeon integrada está sem monitor ativo; isso ainda não valida captura/encode AMF.

Após o proprietário encaminhar as portas, TCP 8101 público respondeu e mídia AV1/HEVC passou por UDP público: 658/653 quadros D3D11, pausa respeitada, RTT de rede de 1–3 ms. A rota local ao WAN foi Ethernet. Nenhuma sala estava ativa; retiramos temporariamente os candidatos internos para forçar a rota pública, verificamos `remote non-private` tanto no publicador quanto no espectador e restauramos a configuração ao final. Pod voltou a 1/1 Running e HTTP 200. RTT de rede não mede latência de captura/encode/filas/decode/apresentação.

Preparação do diagnóstico QHD: encoder e publicação agora são inicializados antes de adquirir o primeiro frame DXGI, tanto no CLI quanto na interface. Isso evita reter uma textura emprestada enquanto a rede confirma a publicação. A janela medida pelo CLI começa após preparação; continua incluindo pausas/adaptação por congestionamento. Regressão AV1/WSS passou e GUI publicar/parar/republicar recebeu 1.110 quadros D3D11 na repetição. Um primeiro smoke GUI terminou sem publicar; a repetição passou, e o diagnóstico foi ampliado para incluir estado de conexão/permissão com token redigido. Investigar inicialização fria caso volte a ocorrer.

## Primeira amostra QHD na RTX 4080 — 13/10/2026

Logs fornecidos pelo proprietário no ZIP de diagnóstico; hardware.json confirma RTX 4080, driver 32.0.16.1714 e monitor 2560×1440 a 239 Hz. O identificador da faixa coincide com o stream.log do receptor. Os arquivos brutos permanecem privados, fora do Git.

| Medição | Resultado |
|---|---|
| Captura / alvo | 2560×1440 / 120 Hz |
| Intervalo de publicação medido | 30,00 s, após preparação |
| Quadros codificados / média | 3.602 / 120,05 FPS |
| Desktop atualizado / polls sem atualização | 87 / 3.515 |
| AcquireNextFrame p50 / p95 / p99 | 0,042 / 0,066 / 0,090 ms |
| Cópia GPU + NVENC p95 | 1,916 ms |
| Bitrate codificado médio | 17,91 Mbps |
| Outbound WebRTC | 58.490 pacotes, 66.884.690 bytes, 3.593 frames sent |
| Receptor | 2.394 quadros, todos texturas D3D11, 2560×1440, AV1 |
| Rota do receptor | UDP, candidato remoto privado, RTT de rede 37 ms |

O SFU registra publicação da faixa às 09:20:47,632 e fechamento do receptor às 09:21:07,929 (America/Cuiaba). O receptor iniciou sua sessão antes do publicador e encerrou seu teste cerca de dez segundos antes do fim da publicação. Houve aproximadamente 20,30 s de sobreposição após publicação; esse intervalo não é a janela exata entre primeiro e último quadro decodificado. Portanto, dividir 2.394 pelos 30 s da sessão não mede FPS do receptor, e a diferença entre contagens dos dois lados não comprova perda de quadros. O log atual não mede essa janela ativa.

A amostra comprova encode QHD a aproximadamente 120 FPS médios e decode QHD por GPU, com conteúdo predominantemente estático (87 atualizações distintas em 30 s). Não comprova 120 atualizações distintas/s, jitter/estabilidade em movimento intenso, apresentação física a 120 Hz ou latência fim a fim abaixo de 100 ms. O RTT 0 ms do publicador também não comprova latência zero; esse contador pode ter precisão/dados insuficientes. Esta amostra usou rota privada no receptor; a validação pública anterior de AV1/HEVC foi feita em 1080p.

Próxima amostra: receptor por 120 s, iniciado primeiro; publicador por 60 s, com movimento contínuo. Registrar janela de chegada dos quadros, intervalos/jitter e apresentação; confirmar caminho público quando o objetivo for WAN. Medir captura até apresentação separadamente do RTT de rede.

## Compatibilidade de publicação RTX 3090 — 13/10/2026

hardware.json enviado pelo proprietário confirma RTX 3090, driver 32.0.16.1714 e monitor 1920×1080 a 144 Hz. O teste AV1 falhou na consulta de preset (NVENC status 8). Essa GPU não possui encoder AV1; o teste deve usar HEVC/H.265. A ponte passou a validar fabricante da GPU de captura, versão da API oferecida pelo driver e lista de codecs NVENC antes de consultar o preset, com erro específico quando AV1 não é suportado. O script de publicação usa HEVC por padrão; a interface mantém seleção manual de codec.

Regressão local AV1 e HEVC → SFU → D3D11VA passou na RTX 5060, incluindo pausa/subscrição. A RTX 3090 não está disponível para execução pelo agente; nova publicação HEVC no PC do amigo ainda precisa de confirmação. Essa alteração não implementa AMF nem muda a rede do servidor.
