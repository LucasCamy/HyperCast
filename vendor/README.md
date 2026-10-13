# Patches locais do SDK

Fontes crates.io com copyrights/notices preservados:

| Crate | Versão | Alteração |
|---|---|---|
| livekit | 0.9.4 | Propaga recusa AddTrack do SFU; verifica codec pré-codificado antes da admissão |
| libwebrtc | 0.3.52 | Expõe textura/slice D3D11 emprestados e contador de superfícies vivas |
| webrtc-sys | 0.3.49 | Decoder D3D11VA AV1/HEVC, buffer GPU nativo, passthrough HEVC Windows |

FFmpeg compartilhado é resolvido explicitamente junto ao executável. O
libwebrtc pré-compilado incorpora outro FFmpeg; vincular avcodec_* diretamente
causava colisão de símbolos/ABI. Não há ToI420/readback no decoder HyperCast.
AVFrame retém a superfície até o fence GPU do presenter.

Ownership: Room → track → stream/sink; tarefa do sink → Shared → último frame;
frame → AVFrame → pool/textura FFmpeg. Shared não retém Room nem tarefa.
Engine manda Disconnect, une a thread, aborta sinks, fecha Room e limpa frames.
Presenter/cache são destruídos na thread da UI. O teste de reentrada verifica
registro de superfícies GPU vazio antes de reconstruir no mesmo processo.
Publicação rejeitada e stop/republish exercitam falha e teardown do encoder.
FFAPI mantém os módulos DLL uma vez por processo, não uma referência adicional
por sessão. Recursos de frame/decoder não são singletons.

O HEVC anunciado para envio Windows se destina ao NVENC externo e a buffers
pré-codificados. Estes forks não são um SDK genérico de encode HEVC de frames
crus. Atualizar upstream exige revisar os patches e repetir integrações GPU,
quota, reentrada e licença do libwebrtc pré-compilado.
