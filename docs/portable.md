# HyperCast para Windows

Abra hypercast.exe. Informe servidor e token de sala, depois Entrar na sala.
Um token de espectador permite assistir; um token de publicador permite enviar
um monitor. Selecione monitor e formato AV1 ou HEVC antes de compartilhar.
HEVC/H.265 é o padrão da interface; AV1 deve ser selecionado em hardware compatível.
Sair encerra a conexão. Minimizar pausa a recepção; restaurar retoma sua seleção.

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
