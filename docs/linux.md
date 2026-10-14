# HyperCast para Linux

O cliente nativo Linux usa o mesmo servidor, chave compartilhada e convites do
Windows. O pacote inicial é um AppImage x86_64, com bibliotecas de mídia incluídas,
para desktops com glibc 2.39 ou superior (base de build: Ubuntu 24.04).
Bazzite recente é o alvo de teste com a RTX 5060 Ti. Não precisa instalar Rust,
GStreamer ou codecs manualmente para executar o AppImage.

## Abrir no Bazzite

Baixe `HyperCast-0.7.2-linux-x86_64.AppImage` na página de versões e abra pelo
Gear Lever. Se preferir abrir diretamente, habilite a permissão de execução nas
propriedades do arquivo. Sem suporte a FUSE, use `--appimage-extract-and-run`.

Entre com a mesma chave do servidor, informe seu nome e convite, ou crie uma sala.
Nesta primeira versão Linux a chave permanece somente na memória da sessão;
ainda não há integração com o cofre de credenciais do desktop.

## Nome e espaço da sala

Seu nome é salvo em `$XDG_STATE_HOME/hypercast/display-name` ou
`~/.local/state/hypercast/display-name` e preenchido na próxima abertura.
**Recolher menu** amplia a área de vídeo; **Abrir menu** restaura os controles.
Duplo clique na transmissão ou no participante que transmite seleciona uma única
tela; **Grid** volta a mostrar várias. A versão Linux já não renderiza prévia local.
A marca **Transmitindo** indica quem envia, independentemente da tela em destaque.

## Tela, janela, vídeo e áudio

**Compartilhar tela/janela** abre o seletor do desktop via XDG Desktop Portal.
Wayland exige PipeWire e um backend de portal que ofereça ScreenCast; KDE/GNOME
oferecem o seletor de monitor/janela. Outros desktops podem oferecer somente
monitores. O aplicativo nunca troca automaticamente uma janela por captura da
tela inteira. **Tela inteira (X11)** é uma alternativa explícita para X11.

**Trocar tela ou janela** abre uma nova seleção mantendo a origem anterior até
o novo pipeline estar disponível. Cancelar a seleção não amplia a captura.
Para encerrar o envio, use **Parar compartilhamento**.

GPU seleciona NVENC moderno (`nvh265enc`/`nvav1enc`) ou VA-API, conforme os elementos
registrados pelo driver. O perfil usa P1, tuning ultra-low-latency, CBR, GOP curto e sem B-frames. HEVC é a
primeira opção para testar compatibilidade. AV1 requer suporte de codificação e
decodificação em todos os clientes que assistirem. **Compatível · HEVC 30 FPS**
usa x265 em CPU, explicitamente, e não equivale ao modo GPU.

**Áudio do sistema** captura o monitor de reprodução via PulseAudio/PipeWire,
com Opus estéreo. Não usa microfone. Durante esse envio, o áudio recebido fica
silenciado para evitar recaptura/eco. O isolamento de áudio de um único processo
Windows não está disponível no Linux nesta versão.

Destaque/Grid, pausa de recepção, volume, tela cheia e limite de três streams
usam a mesma política de subscrição e permissões do servidor. Streams de camada
única fora de destaque ficam pausadas; miniaturas baratas exigem camadas reduzidas.
A reconexão usa até três tentativas do SDK e mantém o último quadro nesse período.

## Desempenho e validação

O receptor aceita HEVC/AV1 com FFmpeg. HEVC/AV1 tentam hardware CUDA/VA-API e têm
fallback de software (dav1d para AV1) quando o caminho de hardware está indisponível. A apresentação
inicial converte quadros para RGBA e envia textura OpenGL. A conversão da captura
pode negociar buffers de CPU. Portanto **não há garantia de zero-copy, QHD a
120 FPS ou latência fim a fim abaixo de 100 ms no Linux** sem medição física.
O modo GPU publica até 120 FPS; o FPS real depende da origem e do compositor.

O build e os testes automatizados usam Ubuntu/WSL e mídia gerada. A validação em
Bazzite, Wayland e NVENC na RTX 5060 Ti precisa ser feita no PC do colega.
O servidor continua o mesmo, sem reinstalação ou alteração de chaves.

Logs do AppImage aberto normalmente: `~/.cache/hypercast/hypercast.log`
(ou `$XDG_CACHE_HOME/hypercast/hypercast.log`). Não envie a chave de acesso ou JWT.

## Desenvolver

Instale Rust 1.92+ e Clang 21+ (necessário pelo libwebrtc pré-compilado), além de
headers GStreamer 1.24+, FFmpeg, OpenSSL, GLib e bibliotecas de desktop. Execute:

```sh
CC=clang-21 CXX=clang++-21 cargo build --locked --features desktop
./target/debug/hypercast --linux-media-test
./target/debug/hypercast --gui
python3 tools/package-linux.py --binary target/release/hypercast --media-prefix /caminho/runtime
```

O teste de mídia gera frames sintéticos; não captura nem grava seu desktop.
O empacotador verifica o SHA-256 da ferramenta AppImage e usa uma lista fixa de
plugins. Os drivers NVIDIA/Mesa, glibc, portal e serviços de áudio ficam no host.
Licenças, versões de pacotes e avisos de terceiros estão dentro do AppImage.


O AppImage incorpora o runtime GStreamer 1.26.10 para os presets modernos da
série RTX 50 e envio AV1. Para reconstruí-lo, use Meson 1.4+ e
`tools/build-linux-media.sh /prefix/absoluto /diretorio/absoluto`; as três fontes
oficiais são verificadas por SHA-256. O arquivo `linux-media-source.tar.gz`
acompanha a versão com essas fontes e o script. No AppImage, **Atualizar** abre a
página de downloads para substituir o arquivo portátil.
