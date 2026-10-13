# HyperCast para Windows

Na versão 0.4.1, seleção de monitor/janela e **Formato de vídeo** ficam na
lateral. **Som recebido** abre volume e silenciamento. Os controles
compartilhar/parar, áudio enviado, pausar recepção e sair ficam fixos na base da
lateral. A região central fica dedicada às telas, com Destaque, Grid e tela cheia.
A lateral permite rolagem em janelas menores ou salas com muitos participantes.

Versão 0.4.0: escolha **Monitor** ou **Janela de aplicativo**, com áudio do
aplicativo opcional. Você pode trocar a origem durante o envio; somente o codec
continua bloqueado. **Tela cheia/F11** amplia a tela em destaque; **Esc/F11** ou
o botão superior retornam. [Detalhes e limitações](window-sharing.md).

Na versão 0.3.2, o volume recebido usa barra visível com preenchimento,
marcador circular e porcentagem ao lado do rótulo. Arraste o marcador ou use
as setas com o controle focado; o silenciamento continua preservando o volume.

Na versão 0.3.1, monitor e formato ficam na faixa acima das telas. Abra a seleção
de monitor para ver as opções com resolução; escolha HEVC/H.265 ou AV1 nos
botões ao lado. Em 0.3.1 essas opções ficavam bloqueadas durante a transmissão;
0.4.0 permite trocar a origem. A barra inferior distingue compartilhar,
parar, pausar recepção, áudio do sistema e sair.

Participantes aparecem em cartões com inicial, nome e estado. Clique em quem
está compartilhando para destacar a tela. Após entrar, o formulário de acesso
é substituído por um resumo da sala. Servidor/token manual ficam em
**Servidor e acesso avançado** na entrada.

Versão 0.3.0: marque **Áudio do sistema** antes ou durante o
compartilhamento para enviar som de jogos, vídeos e programas. A opção começa
desativada; não captura microfone. Parar a tela também para o áudio.
**Silenciar áudio recebido** interrompe a subscrição de áudio; **Volume**
ajusta a reprodução local. Destaque recebe o som da tela escolhida; Grid mistura
o som das telas visíveis. Pausar recepção ou minimizar pausa vídeo e áudio.

Áudio do sistema requer Windows 11 ou build 20348+ para excluir a reprodução
do próprio HyperCast e evitar eco. Windows 10 comum pode compartilhar vídeo
sem áudio. A captura usa PCM estéreo 48 kHz e WebRTC/Opus até 128 kbps.
Nenhum driver virtual, microfone ou arquivo de gravação é necessário.

Abra hypercast.exe. O servidor público já vem preenchido. Informe nome (letras
sem acentos, números, hífen ou sublinhado) e convite da sala, depois Entrar na sala.
O aplicativo obtém automaticamente um JWT curto; ninguém precisa gerar um token
manual para cada entrada. O convite padrão de amigos permite assistir e compartilhar.
Todos podem entrar (até doze participantes); o SFU aceita até três telas simultâneas,
incluindo publicações em preparação. Ao parar uma tela, outra pessoa pode iniciar.
Selecione monitor e formato AV1 ou HEVC antes de compartilhar.
Convites somente de espectador permanecem disponíveis ao operador. O modo
Token manual (avançado) continua acessível para diagnóstico.
HEVC/H.265 é o padrão da interface; AV1 deve ser selecionado em hardware compatível.
Sair encerra a conexão. Minimizar pausa a recepção; restaurar retoma sua seleção.
Convites não são salvos em disco pelo cliente. Apenas um identificador aleatório
do dispositivo é persistido em LocalAppData/HyperCast/client-id, para que reentrar
com o mesmo convite não consuma outra utilização. Nome e convite são informados
novamente após fechar o aplicativo.
O botão Forçar TURN/TLS (teste), marcado antes de entrar, usa somente relay.
Normalmente deixá-lo desmarcado para permitir UDP direto.

Grid recebe até três telas. Destaque recebe a escolhida e pausa as outras;
miniaturas guardam o último quadro visto. Clique no participante ou no nome
da miniatura para mudar de foco. Pausar recepção cancela as subscrições.

O ZIP inclui as bibliotecas de mídia. Não requer Rust, Python, FFmpeg instalado
ou Visual C++ Redistributable no computador do usuário. Requer Windows 10
2004+ x64 e driver gráfico atualizado. Publicação requer NVIDIA com NVENC e
driver 610+; recepção usa D3D11VA com suporte de hardware ao codec negociado.

Este é um build de desenvolvimento sem assinatura Authenticode. 1440p a
120 FPS e latência abaixo de 100 ms são metas ainda não comprovadas.
Não compartilhe API secret com os clientes; distribua apenas tokens de sala.

A interface salva logs automaticamente em `%LOCALAPPDATA%/HyperCast/diagnostics`,
nos arquivos `gui-*.log`. Cada arquivo tem limite de 1 MiB; são mantidos até
16 arquivos da interface. Pastas criadas pelos scripts não são removidas.
O botão **Copiar diagnóstico** copia o log da execução atual para enviar ao
suporte. Não grava imagens nem tokens. Inclui versão, configuração de captura,
contadores, rota ICE, RTT de rede e erros. RTT não mede latência fim a fim.
Falhas de mudança de resolução/device disparam até três tentativas de reconstrução
do pipeline. Formatos não suportados não são repetidos automaticamente.

Na versão 0.2.1, a captura HDR `R16G16B16A16_FLOAT` (format=10) é convertida
na GPU para SDR com compressão de highlights e curva sRGB. O vídeo enviado
continua sendo SDR. A parada encerra o transceiver RTP para liberar a publicação
anterior no SFU. Um erro de publicação duplicada agora tem mensagem distinta
do limite de três telas.

Para diagnóstico, abra PowerShell na pasta do ZIP e execute
`./Test-Streaming.ps1 -Role Viewer` ou `./Test-Streaming.ps1 -Role Publisher`.
Primeiro extraia TODO o ZIP. Para manter a janela aberta em caso de erro,
use Test-Publisher.cmd ou Test-Viewer.cmd na pasta extraida. Erros de
preparacao ficam em error.log; erros do aplicativo tambem ficam em stream.log.
O teste de publicacao usa HEVC/H.265 por padrao para compatibilidade.
Para AV1 em hardware compativel, execute ./Test-Streaming.ps1 -Role Publisher -Codec av1.
RTX 20/30, incluindo RTX 3090, nao codificam AV1; na interface escolha HEVC/H.265.
O script pede o token com entrada oculta, conecta ao domínio HyperCast por
30 segundos e salva hardware/contadores em LocalAppData/HyperCast/diagnostics.
Publicador compartilha o monitor selecionado; nenhum arquivo de vídeo é gravado.
Para testar outra tela, use -Adapter e -Monitor conforme hypercast.exe --list.
O log informa o caminho ICE selecionado e RTT de rede; RTT não é latência
fim a fim. Para validar internet, usar cliente em outra conexão sem VPN.
