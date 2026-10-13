# Janelas, troca de origem e tela cheia — 0.4.0

Na interface 0.4.1, escolha **Monitor** ou **Janela de app** na seção
**Compartilhar** da lateral. A lista contém janelas visíveis de aplicativos; use
**Atualizar janelas** depois de abrir um programa. Escolha a janela antes de
compartilhar. O monitor selecionado também define a GPU usada para codificação
de janelas; publicação continua exigindo NVIDIA NVENC.

Durante o compartilhamento, selecione outro monitor ou janela. A publicação
de vídeo permanece na mesma sala e com o mesmo SID; a nova origem começa com
um keyframe. O formato HEVC/AV1 continua bloqueado durante o envio. Preparação
de captura/encoder acontece antes da substituição; uma falha é mostrada na
lateral. A base fixa da lateral identifica a origem efetivamente ao vivo.

**Áudio do aplicativo** captura o processo da janela e seus processos filhos,
em estéreo, sem microfone. Pode incluir outras janelas/abas do mesmo programa;
não isola uma aba de navegador. **Áudio do sistema** continua excluindo a saída
do próprio HyperCast. Na troca de origem, a captura de áudio muda mantendo a
mesma faixa Opus publicada. Áudio por processo exige Windows 11/build 20348+.

O vídeo de janela usa Windows Graphics Capture com frame pool sem dispatcher,
texturas D3D11 e cópias somente na GPU. A borda de captura do Windows permanece.
Redimensionamento recria o encoder e solicita keyframe; dimensões ímpares são
recortadas em até um pixel na direita/base. Limite atual: 3840×2160. Conteúdo
protegido pode aparecer preto; jogos devem expor uma janela capturável (modo
janela/sem bordas). Janelas minimizadas podem congelar. Ao fechar a janela, o
envio termina; não há fallback para a área de trabalho.

Para assistir, selecione o participante em **Destaque** e use **Tela cheia**
ou **F11**. **Esc**, **F11** e o botão superior saem da tela cheia. O modo anterior
Grid/Destaque é restaurado. A barra de saída fica fora do HWND do vídeo para
continuar acessível durante apresentação DirectX.

Reconexões temporárias suspendem o envio, mantêm a captura/publicação e retomam
com keyframe. No reinício completo de WebRTC, o SDK republica tela antes do
áudio, conforme a admissão do SFU. Desconexão definitiva ainda encerra a sessão.

## Verificação

`python -m tests.integration_window_capture` usa mídia sintética de um FFplay
isolado: vídeo/áudio recebidos, resize, janela → monitor → janela com mesmo SID,
tela cheia e encerramento ao fechar a janela. `python -m
tests.integration_signal_reconnect` interrompe somente um proxy TCP do cliente
de teste; confirma retomada de vídeo e áudio no receptor sem reiniciar o SFU.
Credenciais administrativas ficam no processo de teste; clientes recebem JWTs
de cinco minutos. Nenhum arquivo de vídeo/áudio é gravado. Screenshot de QA
é somente da janela HyperCast de teste e permanece fora do Git.

Referências: [Windows Graphics Capture](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture)
e [loopback de áudio por processo](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/).
