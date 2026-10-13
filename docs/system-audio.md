# Áudio do sistema

0.3.0 captura som de programas via WASAPI process loopback, excluindo a árvore
de processos do próprio HyperCast. Não captura microfone nem grava arquivos.
Requer Windows build 20348+ (Windows 11). Sem fallback que recapture a saída
do próprio aplicativo. Referência: [Microsoft Application Loopback](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/).

A faixa `screen_share_audio` usa PCM 48 kHz estéreo/16 bits → WebRTC Opus,
até 128 kbps, DTX desativado e RED habilitado. A captura e WASAPI playout ficam
em threads próprias, sem bloquear a captura DXGI. A fila de envio é 20 ms;
histórico PCM máximo 30 ms. A recepção usa filas limitadas a três frames,
descartando frames antigos quando cheias, e limita o padding da saída WASAPI.
Windows mistura até três saídas em Grid. Não há sincronizador externo para
garantir alinhamento físico áudio/vídeo nem comprovação de latência <100 ms.

Recepção segue foco/visibilidade: Destaque recebe áudio da tela escolhida;
Grid recebe telas visíveis; minimizar, pausar recepção, silenciar ou volume zero
cancelam subscrição de áudio. A opção de publicar é opt-in e pode ser alterada
durante a transmissão. Parar a tela interrompe captura e publicação de áudio.
Troca/hotplug de dispositivo pode exigir desativar e reativar áudio.

O servidor usa as mesmas portas ICE/TURN; nenhuma porta nova foi aberta.
Os codecs permitidos agora incluem `audio/opus`, com `subscription_limit_audio: 3`.
Convites autorizam somente tela e áudio de tela. O patch SFU exige uma tela
reservada do mesmo membro, recusa duplicatas/microfone, e remove áudio pendente
ou ativo ao fechar a tela. Fechamentos de tracks ocorrem fora do lock de pendências.

`tools/deploy_audio_on_node.py` roda como root no nó existente, após importar
imagens `hypercast-sfu:audio` e `hypercast-auth:audio`. Verifica ausência de
participantes, preserva TURN/portas, salva `/etc/hypercast/audio-rollback.json`
modo 0600 e restaura configurações/imagens se a atualização falhar. O snapshot
contém dados sensíveis; não copiá-lo para clientes nem para Git. Reinstalação
do nó exige reimportar as imagens ou publicar/configurar pull no GHCR.

Testes: `tests/system-audio.ps1` verifica exclusão da reprodução no próprio
processo e limites PCM. Executar com outros programas sem áudio. A integração
`python -m tests.integration_system_audio` gera tons distintos L/R em FFplay,
publica/recebe via TURN TLS, confirma PCM não silencioso e canais diferentes,
WASAPI playout, parada/republicação e silenciar/retomar. No teste, saída do
receptor reproduz zeros para evitar realimentação entre dois clientes distintos
no mesmo PC; o áudio decodificado é verificado antes desse silenciamento.
Não são gravados áudios/vídeos. Apenas metadados/contadores vão para diagnósticos.
