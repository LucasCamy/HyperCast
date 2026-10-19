# HyperCast para Linux

O cliente nativo Linux usa o mesmo servidor, chave compartilhada e convites do
Windows. O pacote inicial é um AppImage x86_64, com bibliotecas de mídia incluídas,
para desktops com glibc 2.39 ou superior (base de build: Ubuntu 24.04).
Bazzite recente é o alvo de teste com a RTX 5060 Ti. Não precisa instalar Rust,
GStreamer ou codecs manualmente para executar o AppImage.

## Abrir no Bazzite

Baixe `HyperCast-0.9.0-linux-x86_64.AppImage` na página de versões e abra pelo
Gear Lever. Se preferir abrir diretamente, habilite a permissão de execução nas
propriedades do arquivo. Sem suporte a FUSE, use `--appimage-extract-and-run`.

Entre com a mesma chave do servidor, informe seu nome e use **Escolher sala**, ou crie uma sala.
Nesta primeira versão Linux a chave permanece somente na memória da sessão;
ainda não há integração com o cofre de credenciais do desktop.

## Nome e espaço da sala

Seu nome é salvo em `$XDG_STATE_HOME/hypercast/display-name` ou
`~/.local/state/hypercast/display-name` e preenchido na próxima abertura.
**Recolher menu** amplia a área de vídeo; **Abrir menu** restaura os controles.
Duplo clique na transmissão ou no participante que transmite seleciona uma única
tela; **Grid** volta a mostrar várias. A versão Linux já não renderiza prévia local.
A marca **Transmitindo** indica quem envia, independentemente da tela em destaque.

Para trocar, abra **Escolher sala** durante a sessão e entre em outra. O aplicativo
encerra o envio e a conexão anteriores em segundo plano antes de conectar à nova
sala. O compartilhamento precisa ser iniciado novamente na sala de destino.
Respostas atrasadas e estado de mídia ocupado não bloqueiam o desenho da interface.
Os testes dessa recuperação usam Ubuntu/WSL; a confirmação visual com NVIDIA e
Wayland no Bazzite ainda depende do PC do colega.

## Tela, janela, vídeo e áudio

Em **Formato de vídeo**, escolha o limite **12, 25 ou 40 Mbps** antes de iniciar;
o padrão é 25 Mbps. O modo Compatível permanece limitado a 12 Mbps/30 FPS.
Resolução e alvo de FPS do modo GPU são preservados. Veja [limites e overhead](video-quality.md#limite-de-upload).

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

Formato de vídeo e volume ficam em seções recolhíveis, com o mesmo padrão de
botões e controles do Windows. Atualização e Diagnóstico usam janelas com bordas
e espaçamento comuns. No Linux, Atualização abre o download do AppImage; não
instala um pacote nem declara automaticamente que há uma versão mais recente.
A barra da sala mostra [indicadores medidos](video-quality.md#indicadores-na-interface).

O encerramento após escolher uma tela no Bazzite ainda precisa do log do PC
afetado; as mudanças visuais de 0.8.6 não constituem uma correção dessa falha.

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

Logs da interface do AppImage (aberto normalmente ou com `--gui`):
`~/.cache/hypercast/hypercast.log` (ou `$XDG_CACHE_HOME/hypercast/hypercast.log`).
A partir da 0.8.9, o lançador registra início e código de saída mesmo quando o
processo filho encerra por um sinal; a captura registra etapas do portal,
PipeWire e encoder, sem imagens nem credenciais. Os comandos de diagnóstico
via CLI continuam escrevendo no terminal. Não envie a chave de acesso ou JWT.

Se o aplicativo fechar, envie o log completo dessa execução, incluindo as linhas
`Linux capture: stage=...` e `HyperCast launcher: exit_code=...`. Se o sistema
encerrar também o lançador (por exemplo, falta de memória), a linha de saída
pode não existir; nesse caso é necessário consultar o journal do sistema.
Um aviso sobre `libnvrtc.so` sozinho não comprova a causa do fechamento.

A 0.8.10 corrige uma falha de empacotamento que podia fechar o aplicativo ao
iniciar qualquer captura pelo portal, inclusive no modo HEVC por software.
O `client.conf` incorporado exigia os módulos PipeWire `client-device` e
`session-manager`, que não estavam no AppImage. O plugin PipeWire 1.0.5 não
tratava o contexto nulo e causava SIGSEGV. O pacote inclui os seis módulos
obrigatórios e recusa a geração se algum estiver ausente.

O teste `python3 -m tests.integration_pipewire_appimage --appimage /caminho/AppImage`
usa um daemon PipeWire privado, sem dispositivos, e abre/fecha três conexões por
descritor de arquivo, como no portal. Ele reproduz a falha no pacote 0.8.9 e
verifica a inicialização/limpeza do pacote corrigido. Não captura tela nem áudio;
a seleção real no portal do Bazzite ainda depende de validação nessa máquina.

A 0.8.11 define os limites de formato/resolução antes de iniciar o pipeline.
O callback de formato recebido passa a apenas validar e registrar o tamanho;
ele não altera mais o capsfilter durante a negociação do PipeWire. Essa alteração
provocava renegociação da origem e podia interromper o fluxo ou bloquear a
inicialização. O conversor negocia dimensões pares dentro dos limites do encoder,
mantendo a proporção. O teste acima aceita `--capture-test-binary /caminho/teste`
para conectar uma fonte PipeWire sintética ao pipeline de captura real e exigir
dez quadros codificados. O binário é gerado com `cargo test --features desktop
--lib --no-run`; o teste de Rust se chama `pipewire_first_encoded_frames`.

A 0.8.12 solicita de dois a quatro buffers ao `pipewiresrc` e ativa
`always-copy`, desacoplando os buffers emprestados do compositor das filas do
encoder. O padrão do plugin incluído exigia no mínimo oito buffers, enquanto
o [KWin oferece de dois a quatro](https://github.com/KDE/kwin/blob/master/src/plugins/screencast/screencaststream.cpp).
O caminho portátil Linux faz uma cópia adicional na CPU; não representa uma
implementação zero-copy e seu custo precisa ser medido na máquina de destino.
O log passa a registrar formato de pixel, tipo de memória e política de buffers.

Para testar também um produtor cliente que aloca seus próprios MemFd, oferece
BGRA com FPS variável e limita o pool a quatro buffers, compile
`tests/pipewire_limited_source.c` com os headers/lib de `libpipewire-0.3` e passe
`--limited-source /caminho/produtor` junto com `--capture-test-binary` no teste.
Esse produtor só gera pixels sintéticos e verifica a liberação dos seus buffers.
Os testes com o servidor PipeWire 1.0.5 não reproduziram o EINVAL do Bazzite;
o ajuste de compatibilidade ainda depende da confirmação no compositor afetado.

A 0.8.13 mantém a publicação ao trocar de janela/monitor com o mesmo codec,
FPS e orçamento. A captura nova substitui a anterior somente depois do primeiro
quadro codificado; o encerramento do pipeline anterior ocorre fora do loop de
mídia. Uma troca de codec, modo de encoder (30/120 FPS) ou orçamento renova a
conexão Linux da sala, com breve interrupção também da recepção/áudio, para não
reutilizar a negociação anterior. Não é necessário sair manualmente da sala.
Pedidos de keyframe são enviados de forma assíncrona ao encoder. A fila
codificada aplica contrapressão com tamanho limitado; apenas a fila de quadros
brutos descarta imagens antigas, preservando as referências HEVC/AV1.

No NVENC, formatos BGRA/RGBA do portal podem entrar diretamente no encoder e
evitar a conversão de cor forçada na CPU. Continua havendo cópia MemFd e upload
para a GPU; redimensionar pode exigir processamento adicional na CPU.
A cada cinco segundos, `Linux capture rates` registra FPS de origem,
entrada do encoder, saída codificada e entrega ao loop de rede. `Linux publish
rate` registra o bitrate solicitado pelo WebRTC. Fonte estática do compositor
pode legitimamente emitir poucos quadros; esses números não garantem 120 FPS.

O teste `publisher_source_and_codec_switches` gera imagens e exige que outro
cliente decodifique cada troca, preservando SID apenas na troca compatível.
`tests/integration_linux_publisher_switch.py` executa esse teste na sala isolada
do ambiente de desenvolvimento Windows/WSL, com as bibliotecas do AppImage.

A 0.8.14 acrescenta resolução de envio Nativa, Full HD (máximo 1920×1080)
e Quad HD (máximo 2560×1440), além de 30, 60 ou 120 FPS. As opções ficam na
seção de formato antes de compartilhar. O redimensionamento mantém a proporção,
não amplia fontes menores e não muda a resolução do monitor. Por exemplo,
3440×1440 enviado como Full HD resulta em aproximadamente 1920×804.
O modo HEVC por software continua limitado a 30 FPS. Pare o compartilhamento
para alterar esses limites; a troca de janela mantém os limites selecionados.

Sair cancela a admissão HTTP ou a conexão WebRTC pendente por um sinal separado
da fila de comandos. Trocas rápidas preservam uma única tarefa de encerramento
e apenas o último destino escolhido. Sair durante essa espera cancela também
o próximo ingresso. Os testes simulam servidores que não respondem, encerram
conexões durante o handshake e verificam cancelamento e liberação dos workers.

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
