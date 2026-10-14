# Componentes redistribuídos

HyperCast: MIT, código em https://github.com/LucasCamy/HyperCast.
LiveKit Rust 0.9.4, libwebrtc 0.3.52 e webrtc-sys 0.3.49: Apache-2.0.
Os fontes correspondentes e alterações estão em vendor/ neste repositório.
No LiveKit Rust, a republicação após reinício completo de RTC é sequencial:
vídeo de tela antes do áudio associado, para respeitar a admissão do SFU.
LiveKit Server 1.13.9: Apache-2.0; commit 6fbe7f7e648aab4b2f734d6044a416bc2cbc482c;
fontes em https://github.com/livekit/livekit e patch em infra/sfu.

FFmpeg 9.0.2-27-g7e93014c5b: LGPL-3.0, bibliotecas compartilhadas não modificadas
do build BtbN LGPL, com --enable-version3 e sem --enable-gpl/--enable-nonfree.
Fontes: https://github.com/FFmpeg/FFmpeg/tree/7e93014c5b.
Receitas, patches e dependências: https://github.com/BtbN/FFmpeg-Builds.
Binário exato: release autobuild-2026-10-12-13-12, variante
ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0.zip;
SHA256 de3f3b16b3dbf2038a56878aa35795f48bd220f01479697e008470773cecba5b.
A licença e configuração são incluídas em licenses/. DLLs são carregadas
dinamicamente e podem ser substituídas por builds compatíveis do usuário.
Não há restrição adicional de engenharia reversa para depurar modificações
dessas bibliotecas. Veja COPYING.LGPLv3 e COPYING.GPLv3 distribuídas juntas.

Header nvEncodeAPI.h: MIT, FFmpeg/nv-codec-headers, commit
eddcea9e27f6b772057c9b3f87de2cc1737faffc; copyright original no próprio header.
O driver NVIDIA é fornecido pelo fabricante, não redistribuído pelo instalador.

Outras dependências Rust e respectivas versões/licenças constam do Cargo.lock
e dos manifests. A biblioteca WebRTC pré-compilada possui componentes BSD/MIT/
Apache; a NOTICE do fornecedor é incluída junto com o aplicativo.

AMF SDK 1.5.2: headers MIT, copyright AMD preservado. Fonte fixada em
https://github.com/GPUOpen-LibrariesAndSDKs/AMF/tree/8c648005e07d4309033282bfd9947df2c7e76104;
headers em native/vendor/amf. AMF-LICENSE.txt acompanha os pacotes.
O runtime AMD amfrt64.dll vem do Adrenalin e é carregado somente de System32;
não é redistribuído. NVIDIA continua usando NVENC no dispositivo da captura.


## Linux AppImage

O pacote Linux inclui GStreamer, plugins e bibliotecas FFmpeg da base Ubuntu
24.04. As licenças e versões exatas estão em `usr/share/hypercast/licenses`
dentro do AppImage, incluindo componentes LGPL/GPL (por exemplo, x265).
A versão publicada também fornece os pacotes de fonte correspondentes dessas
bibliotecas em `HyperCast-0.7.0-linux-corresponding-source.tar.gz`, com manifestos
e hashes, e os scripts de build/empacotamento permanecem neste repositório.
Os drivers NVIDIA/Mesa e a glibc são usados do sistema e não são empacotados.

O runtime GStreamer 1.26.10 compilado para NVENC moderno tem suas fontes exatas
e script em `HyperCast-0.7.0-linux-media-source.tar.gz`, além das fontes das
bibliotecas Ubuntu. Os hashes dos dois arquivos são publicados junto ao AppImage.
