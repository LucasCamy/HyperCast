# Validação — 12/10/2026

| Verificação | Resultado |
|---|---|
| Compose principal e override Desktop: `config --quiet` | Passou |
| LiveKit v1.13.9 iniciando com usuário 10001 e configuração fornecida | Passou |
| HTTP local e contêiner de healthcheck | Saudável |
| JWT administrativo válido aceito pelo RoomService | Passou |
| JWT adulterado recusado pelo servidor | HTTP 401 |
| Emissão de três reservas, rejeição da quarta e token de espectador | Passou |
| Sete testes Python, incluindo 12 pedidos concorrentes | Passaram |
| `cargo check --locked --target x86_64-pc-windows-msvc` no Docker Rust 1.90 | Passou |
| `cargo test --locked` no Linux, política de subscrição | 1 teste passou |
| C++ MSVC 14.51.36231 e Windows SDK 10.0.26100 instalados | Instalador Microsoft terminou com exit code 0 |
| FFmpeg/ffprobe | Pacote Gyan.FFmpeg atualizado para 9.0.2 via WinGet, hash verificado |
| Build nativo com `nvenc,livekit-network` | Passou com CRT estático |
| Testes nativos com ambas as features | 2 passaram: política de subscrição e transporte TLS do token |
| Captura DXGI na RTX 5060 | Executada em 1920×1080 |
| HEVC local, teste de 15 s | 119,05 frames/s; aquisição p95 0,185 ms; cópia+encode p95 4,910 ms |
| AV1 local, teste de 15 s | 119,57 frames/s; aquisição p95 0,091 ms; cópia+encode p95 3,232 ms |
| Streams HEVC e AV1 locais de 5 s | ffprobe confirmou 1920×1080/yuv420p/sem B-frames; ffmpeg decodificou sem erros |
| Cliente Rust entrando em sala | Passou, token de espectador, sem auto-subscribe |
| Publicação AV1 via LiveKit, teste de 15 s | 119,11 frames codificados/s; WebRTC reportou 14.603 pacotes, 16.158.049 bytes e 1.784 frames enviados |
| Recepção por espectador, decoder, renderização e instalador | Ainda não implementados |
| Quota rígida de faixas no SFU | Pendente; emissor limita vagas de publicadores |
| 1440p@120, aquisição ≤4 ms e latência <100 ms | Não medidos |

O ambiente de teste do servidor permanece local, com o override `docker-compose.dev.yml` e sinalização/IP anunciado em `127.0.0.1:8100`. Nenhum recurso remoto do homelab foi alterado. Foram usados tokens temporários e nomes de sala de diagnóstico. A GPU é uma RTX 5060 de 8 GB com driver 610.88; Rust nativo 1.99.0. As amostras foram coletadas com pouco movimento no desktop e durante compilação no host; não são um benchmark controlado de conteúdo 1440p@120.

O modo single-PC do SDK 0.9.4 gerou `multiple conflicting ice-ufrag values` no servidor 1.13.9. A publicação testada usa `single_peer_connection=false`, com dois transportes para o mesmo SFU. A primeira checagem C++ também detectou divergência de CRT (/MD vs /MT), corrigida com runtime estático consistente no Rust e na ponte.

O número de frames NVENC inclui repetição da última imagem própria da GPU: não representa 120 atualizações distintas do desktop. O throughput de encode e os contadores de envio WebRTC não medem latência fim a fim, sucesso de decode em espectador ou performance de três publicadores. HEVC sobre WebRTC, AMF, camadas reduzidas e perda/jitter ainda precisam de validação.

Para repetir a verificação Rust nesta máquina sem linker C++:

```powershell
./tools/check-rust.ps1
```

A checagem cruzada valida tipos e APIs Windows; não faz link nem executa o código DXGI. Medir apenas `AcquireNextFrame` também não mede conversão de cor, encoder, rede ou apresentação.

Para repetir o build/test nativo:

```powershell
./tools/cargo-native.ps1 test --locked --features 'nvenc,livekit-network'
```
