# HyperCast

Base incremental de compartilhamento de tela nativo Windows com SFU LiveKit centralizado. Infraestrutura, captura DXGI, encoder NVENC HEVC/AV1, publicação pré-codificada e subscrição seletiva estão implementados. AV1 foi exercitado do publicador até um espectador com frames decodificados. O receptor atual é um diagnóstico sem janela; renderização D3D11 e UI ainda não estão implementadas. 2560×1440 a 120 FPS e latência fim a fim <100 ms continuam sendo metas, não resultados comprovados.

## Infraestrutura

O `.env` local já foi gerado com chaves aleatórias. Em outra máquina:

```powershell
python tools/init_env.py
docker compose config --quiet
docker compose up -d
```

`init_env.py` nunca sobrescreve credenciais existentes. Proteja `.env` com permissões locais; não o entregue aos clientes. O gerador de tokens é uma ferramenta administrativa local, não um endpoint público.

O Compose principal usa rede host, destinada a Linux. Para desenvolvimento no Docker Desktop, com mapeamento explícito:

```powershell
docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d
python tools/room_token.py --room amigos --name Lucas --role publisher
python tools/room_token.py --room amigos --name Amigo --role viewer
```

Esses comandos imprimem tokens bearer com validade de cinco minutos. Distribua-os por um canal seguro. Espectadores podem subscrever e não podem publicar mídia/dados. Publicadores só podem publicar fonte `screen_share`.

Para produção, altere `rtc.node_ip` em `livekit.yaml` para o IP alcançável do servidor. O padrão `192.168.15.19` vem do homelab; não representa esta estação Windows. O override de desenvolvimento anuncia loopback por padrão e expõe a sinalização apenas em loopback, adequado ao teste cliente/SFU nesta mesma máquina. `HYPERCAST_DEV_NODE_IP` pode ajustar o IP anunciado para outro cenário de desenvolvimento; também será necessário ajustar o bind HTTP do override para acesso de outra máquina. Em produção, use HTTPS/WSS com proxy reverso e mantenha o acesso administrativo restrito. `LIVEKIT_URL` é a URL HTTP(S) administrativa do SFU; o cliente recebe uma URL WS(S).

| Porta | Transporte | Uso |
|---|---|---|
| 8100 | TCP | Sinalização WebSocket e RoomService HTTP |
| 8101 | TCP | Fallback ICE/TCP |
| 8102–8105 | UDP | RTP/RTCP, mux em quatro portas |

Não combine `udp_port` com `port_range_start/end`. A faixa de quatro portas segue os quatro vCPUs documentados. Ajuste conforme o host real. O arquivo `infra/90-hypercast.conf` oferece tetos de buffer de socket para aplicar manualmente no host Linux com `sysctl`; nenhum ajuste foi aplicado nesta máquina.

O buffer de vídeo de 1024 pacotes é para retransmissão NACK: a 45 Mbps e payloads de ~1200 bytes, retém aproximadamente 218 ms de histórico. Não representa atraso de playout. Bitrate e FPS serão configurados no encoder. Controle de congestionamento permanece ativo para evitar filas crescentes. Sem TURN nesta fase, redes que bloqueiam UDP e ICE/TCP podem não conectar.

## Admissão e múltiplas telas

`HYPERCAST_MAX_PUBLISHERS` aceita 1–3. O emissor usa SQLite com transação `BEGIN IMMEDIATE`, reserva identidades fixas por sala e consulta participantes ativos antes de emitir um token de publicação. A quarta solicitação é rejeitada; espectadores continuam permitidos até o limite de 12 participantes da sala. Reservas não utilizadas expiram em cinco minutos. A expiração do JWT não encerra uma sessão já conectada: a consulta ao SFU mantém essas vagas ocupadas. Reduzir o limite exige esvaziar publicadores/reservas incompatíveis.

Todos os emissores devem usar este mesmo banco local; não replique o emissor com bancos independentes. Não apague o banco para liberar vagas enquanto existirem tokens válidos. As identidades de vaga identificam sessões, enquanto `name` contém o nome do amigo.

**Limitação da etapa:** isto limita participantes autorizados a publicar, não o número de faixas de um cliente modificado. O LiveKit padrão não fornece quota rígida de faixas por sala em YAML. `limit.num_tracks`/`bytes_per_sec` são limites de alocação do nó e não resolvem esse requisito. Antes de disponibilizar a publicação no passo 3, implementar admissão síncrona no SFU para contar faixas ativas e pendentes e rejeitar a quarta, ou uma integração de publicação cuja fronteira de confiança imponha uma faixa por identidade. Webhook que remove uma faixa depois da publicação não equivale à rejeição antecipada.

`src/subscription.rs` define a política Grid/Destaque, aplicada às publicações remotas por `src/receiver.rs`: invisível → desabilitar e cancelar subscrição, principal → completa, secundária → camada baixa quando a publicação sinaliza simulcast. Sem camadas reduzidas, a secundária é pausada. O receptor seleciona somente fontes de compartilhamento de tela e limita a três subscrições. O SFU não transcodifica: miniaturas com bitrate reduzido exigem camadas AV1 SVC, simulcast ou outro fluxo produzido pelo encoder. Reduzir apenas a renderização não reduz rede/decodificação. O encoder atual publica uma camada única.

## Captura DXGI

Rust usa `windows-rs` e obtém `ID3D11Texture2D` diretamente de `AcquireNextFrame`, sem `Map`, staging texture ou readback CPU. O dispositivo D3D11 é criado no adaptador dono do monitor selecionado. Um guard exclusivo libera o frame automaticamente, inclusive ao sair por erro. Após `ReleaseFrame`, o conteúdo da superfície DXGI não pode ser usado pelo encoder.

A ponte inicial `native/nvenc.cpp` usa uma textura própria e encode síncrono. Copia a superfície DXGI em GPU, aguarda um query/event de conclusão e registra a textura BGRA diretamente no NVENC. A conversão de cor fica a cargo do hardware/driver. O frame DXGI só é liberado depois dessa operação. Isso evita transferência de imagem bruta para RAM, mas inclui uma cópia GPU. Para maior paralelismo, a evolução será um pool assíncrono limitado, com sincronização e descarte de frames atrasados.

O encoder usa preset P1, tuning ultra-low-latency, CBR, GOP de um segundo, `frameIntervalP=1` (sem B-frames) e lookahead desligado. HEVC usa perfil Main; AV1 é outra opção. Apenas o bitstream comprimido cruza para a RAM para packetização WebRTC. Em desktops estáticos, a última textura própria é reutilizada a cada tick, mantendo o fluxo ativo sem acessar uma superfície DXGI liberada. A DLL NVENC é carregada somente de System32. O header incluído exige driver NVIDIA 610+; AMF ainda não está implementado.

O pacer usa timer Windows de alta resolução e deadlines absolutos, descartando ticks perdidos. Não faz busy loop. `DXGI_ERROR_WAIT_TIMEOUT` é normal em desktops estáticos; `ACCESS_LOST` tenta recriar a duplicação no mesmo dispositivo. Perda do dispositivo ou troca de adaptador exige reconstruir o pipeline. Monitores rotacionados são rejeitados explicitamente. Cursor separado, HDR, seleção visual de monitores e composição GPU ainda não estão implementados.

Com Rust e Visual Studio Build Tools (C++ + Windows SDK), em um terminal de desenvolvimento:

```powershell
./tools/cargo-native.ps1 build --release --features nvenc
./target/x86_64-pc-windows-msvc/release/hypercast.exe --list
./target/x86_64-pc-windows-msvc/release/hypercast.exe --adapter 0 --output 0 --fps 120 --seconds 30 --encode --codec av1
```

O diagnóstico mostra dimensões nativas, quantidade de frames, atualizações do desktop e p50/p95/p99 da aquisição. Ele não escala o monitor para 1440p nem aumenta sua taxa de atualização. Use uma tela a 120 Hz com conteúdo em movimento. Para validar latência fim a fim, será necessário o pipeline completo e medição entre origem e apresentação.

Rust foi instalado de forma isolada em `.tools`, sem alterar PATH. O workload C++ do Visual Studio, Windows SDK e ferramentas FFmpeg estão instalados nesta máquina. `cargo-native.ps1` carrega o ambiente MSVC e seleciona explicitamente o alvo Windows para manter o runtime CRT estático compatível com a biblioteca WebRTC. A captura e o encoder já foram executados na RTX 5060, nos monitores disponíveis de 1920×1080. Consulte os números e limitações em `docs/validation.md`.

## Publicação nativa

Build com as duas features:

```powershell
./tools/cargo-native.ps1 build --features 'nvenc,livekit-network'
```

Teste administrativo local, obtendo um JWT sem colocá-lo na linha de comando do cliente:

```powershell
$env:HYPERCAST_ROOM_TOKEN = python tools/room_token.py --room amigos --name Lucas --role publisher
try {
    ./target/x86_64-pc-windows-msvc/debug/hypercast.exe --adapter 0 --output 0 --encode --codec av1 --fps 120 --seconds 30 --publish-url ws://127.0.0.1:8100
} finally {
    Remove-Item Env:HYPERCAST_ROOM_TOKEN
}
```

O cliente envia access units completas via `NativeVideoSource::new_encoded` / `capture_encoded_frame`, com `VideoEncoderBackend::PreEncoded`. Pedidos de keyframe geram IDR; pedidos de bitrate/FPS reconfiguram o NVENC e o pacer, respeitando o teto escolhido. Target zero pausa encode. Congestionamento pode reduzir FPS/bitrate: o teto de 120 FPS não é uma garantia em rede saturada. A implementação ainda não cria camadas/miniaturas nem altera resolução.

O SDK opera com `auto_subscribe=false`. Foram usados dois transportes WebRTC por cliente porque o modo single-PC apresentou erro de ICE/SDP contra a versão fixada do SFU. Ambos continuam conectados ao servidor central. Mudança para estado de reconexão encerra o envio com erro; recuperação automática completa é trabalho pendente. WebRTC aceita WSS e permite WS somente em loopback. O token fica em `HYPERCAST_ROOM_TOKEN`; o cliente nunca precisa da chave secreta de API.

Para apenas testar entrada na sala, use um token de espectador e `--connect-url ws://127.0.0.1:8100 --seconds 5`. Essa opção não subscreve nem renderiza vídeo.

## Recepção seletiva sem janela

Em outro terminal, com o publicador ativo e um token de espectador:

```powershell
$env:HYPERCAST_ROOM_TOKEN = python tools/room_token.py --room amigos --name Espectador --role viewer
try {
    ./target/x86_64-pc-windows-msvc/debug/hypercast.exe --receive-url ws://127.0.0.1:8100 --view-mode spotlight --focus hc-screen-1 --seconds 30
} finally {
    Remove-Item Env:HYPERCAST_ROOM_TOKEN
}
```

`--view-mode grid` seleciona até três telas. No Destaque, `--focus` usa a identidade da vaga de publicação; sem esse argumento, a primeira identidade ordenada é escolhida. `--pause-after 6` permite exercitar a pausa global e o cancelamento de subscrições após seis segundos. As opções de layout ainda são fixas durante uma execução; a futura UI precisará fornecer controles de troca de foco.

O diagnóstico usa o decode do SDK e um sink com fila de um frame, descartando os mais antigos quando necessário. Ativa zero playout delay antes de inicializar o runtime. Conta frames e dimensões sem convertê-los para I420/RGBA. Isso não comprova D3D11VA/NVDEC nem entrega uma textura pronta para swapchain: a ponte para decode/render GPU e a visualização ainda precisam ser implementadas. A execução termina com erro se nenhuma tela tiver sido recebida no intervalo.

Teste automático local do caminho AV1 e da pausa, exigindo NVIDIA/DXGI/SFU:

```powershell
python -m tests.integration_media
```

O teste usa tokens temporários nos ambientes dos subprocessos, cria uma sala exclusiva e não grava o conteúdo da tela em arquivo.

`--dump caminho.hevc` ou `--dump caminho.obu` salva o stream local comprimido para diagnóstico; exige `--encode` e recusa sobrescrever arquivo existente. Essas gravações podem conter a tela compartilhada. FFmpeg foi usado somente para verificar a decodificação dos arquivos, não como caminho de captura/encode do aplicativo.

## Verificação

```powershell
python -m unittest discover -s tests -v
python -m tests.integration_tokens
```

O segundo comando exige SFU em loopback na porta 8100. Verifica autenticação administrativa real, rejeição de assinatura adulterada e emissão limitada de tokens; não testa publicação de RTP ou múltiplas faixas conectadas. Os testes nativos adicionais estão registrados em `docs/validation.md`.

## Próximas etapas

1. Admissão rígida de faixas no SFU, pool GPU assíncrono limitado, implementação AMF e recuperação de dispositivo/reconexão.
2. Validar publicação HEVC sobre WebRTC (o encode HEVC local já foi validado), múltiplos publicadores, perda/jitter, camadas reduzidas e captura 1440p@120 real.
3. Ponte de decodificação GPU, swapchain D3D11 flip-discard e controles de layout/foco. UI leve com egui sobre integração nativa de apresentação; não transferir frames via WebView/CPU.
4. Instalador autônomo com DLLs necessárias, checagem de GPU/codec, desinstalação e assinatura. O usuário final não precisará de Rust nem Visual Studio.

Referências: [configuração LiveKit](https://github.com/livekit/livekit/blob/v1.13.9/config-sample.yaml), [publicação pré-codificada](https://docs.livekit.io/transport/media/pre-encoded/), [Desktop Duplication](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api), [análise do homelab](docs/homelab.md).

## Versionamento e CI

Repositório: [LucasCamy/HyperCast](https://github.com/LucasCamy/HyperCast), branch `main`. `.env`, tokens, banco de admissão, toolchain local, artefatos compilados e logs ficam fora do Git. `Cargo.lock`, fontes e header NVENC com licença original são versionados. O helper MSVC usa Rust local em `.tools` quando disponível ou o `cargo` instalado no PATH em um clone novo.

O workflow `CI` verifica formatação e build/test nativo no Windows, além dos testes Python e da autenticação contra um SFU temporário no Linux. Os testes de captura/encode/decode que exigem GPU permanecem locais; runners hospedados não substituem o hardware alvo.
