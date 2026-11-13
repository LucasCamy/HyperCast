# Controles da sala

O nome informado fica salvo por usuário. No Windows o arquivo é
`%LOCALAPPDATA%/HyperCast/display-name`; no Linux,
`$XDG_STATE_HOME/hypercast/display-name` ou `~/.local/state/hypercast/display-name`.
Editar o nome substitui o anterior ao sair do campo, entrar na sala ou fechar
normalmente o aplicativo. Credenciais e convites não fazem parte desse arquivo.
No Windows, `HYPERCAST_NAME`, quando definido explicitamente, substitui o preenchimento sem
alterar o cache do usuário. Um cache inválido é ignorado.

**Participantes**, à esquerda do nome da sala, recolhe ou expande o painel lateral.

As suas ações ficam no fim do painel de participantes, num cartão com seu nome
e estado. Ali ficam **Compartilhar tela**, **Parar**, a engrenagem de ajustes,+a câmera, o olho da prévia, **Mais opções** e **Sair da sala**. Com a prévia
ligada, clique no seu avatar para vê-la no palco. **Mais opções** reúne o nome
da sala para copiar, **Trocar de sala**, pausa, reconexão, escala da interface,
**Atualizar** e **Diagnóstico**. Recolhido ou em janelas com menos de 640 pontos,
o painel vira uma coluna com os avatares dos participantes e as suas ações.

**Sair da sala** mantém o acesso ao servidor. Use **Trocar acesso ao servidor**
na tela inicial para encerrar a autenticação.

Em **Compartilhar tela**, escolha se quer incluir o áudio na aba **Fonte** e
ajuste resolução, FPS, codec e limite de upload na aba **Qualidade**. No Linux,
o seletor do sistema abre depois de **Continuar no seletor**. Durante o envio,
a engrenagem ao lado de **Parar** permite trocar a fonte, editar a qualidade e
ligar ou desligar o áudio do compartilhamento. Mudanças de qualidade podem
reiniciar o envio automaticamente. No Linux, confirme a fonte no seletor do
sistema quando o ajuste precisar de uma nova captura. Os indicadores de vídeo
ficam em **Diagnóstico**.

O **olho** controla as prévias locais. O Windows mostra sua tela e sua câmera,
e o Linux mostra sua câmera. Captura, codificação e envio continuam ativos
quando você oculta a prévia.

O alto-falante e o controle de volume sobre o vídeo ajustam o som recebido.
O seletor de compartilhamento mantém a recepção de vídeo e áudio ativa.

Nos participantes, o ponto verde e a indicação de transmissão identificam quem
compartilha mesmo fora do foco. A borda selecionada e **Em destaque** identificam
quem está sendo acompanhado. Recepção pausada não marca alguém como assistido.

Clique no cartão de um participante para colocar a tela dele no palco. Quando
ele só publica câmera, o clique coloca a câmera em destaque. O limite do
servidor permanece três compartilhamentos de tela simultâneos.

## Palco e faixa de vídeos

O palco ocupa toda a área da sala. Sem escolha sua, as telas compartilhadas
dividem o palco: uma ocupa tudo, duas ficam lado a lado e três ou quatro em
grade. Sem telas, a câmera ocupa o palco.

Os demais vídeos visíveis ficam ao vivo numa faixa sobre o palco, um ao lado do
outro.

- Arraste a faixa para qualquer lugar. Ao soltar, ela volta com efeito de mola
  para o encaixe mais próximo: os quatro cantos, o meio de cima, o meio de baixo
  ou o meio de cada lateral. Nas laterais a faixa vira coluna.
- Os cartões encolhem para caber; o que não couber vai para **+N**.
- Clique num cartão ou num vídeo do palco dividido para colocá-lo sozinho no
  palco; o resto vai para a faixa. A troca não reinicia a recepção.
  **Lado a lado** volta à divisão automática das telas.
- Quando uma tela em destaque para, a câmera de quem compartilhava assume o
  palco se estiver ligada e visível pelo filtro.
- Se o palco ficar menor que 480 × 270, a faixa vira uma pílula que lista os
  vídeos para escolher o do palco.
- Clique duas vezes no palco para entrar ou sair da tela cheia.

## Controles sobre o vídeo

Os controles ficam sobre a transmissão e aparecem quando o mouse se mexe. Somem
2 s depois, exceto enquanto o mouse está sobre eles, um menu está aberto ou a
faixa está sendo arrastada. Em tela cheia o cursor também some.

- No canto superior esquerdo de cada vídeo do palco aparece o nome de quem
  transmite.
- A barra inferior reúne **Tudo**, **Telas** e **Câmeras** (o que você quer
  ver); **Palco** ou **Grade**; **Lado a lado** quando houver; som com volume e
  porcentagem; pausa; **Reconectar recepção**; e **Tela cheia**.
- Em palcos estreitos, as escolhas de visualização ficam num menu e o volume num
  popover do alto-falante.
- A faixa sobe para não ficar embaixo da barra enquanto ela aparece.

No Windows, as janelas nativas de vídeo recebem recortes para deixar os
controles visíveis. O cabeçalho da sala mostra o nome, o estado e o acesso ao
painel de participantes. **F11** alterna a tela cheia, **Esc** sai dela, **M**
silencia em tela cheia e **F5** reconecta a recepção.

Vídeos ocultos deixam de ser recebidos. Em **Câmeras**, o áudio das telas
continua. Modo, filtro e encaixe da faixa ficam salvos nas preferências. Na
atualização de versões anteriores à 0.9.7, o palco é o modo inicial.

No Windows, `HYPERCAST_TRACE_STAGE=1` imprime, a cada 2 s, o modo, os vídeos do palco, os
cartões, as assinaturas pedidas e os quadros apresentados por vídeo. Use só em
validação local.

## Verificação

- Testes Rust cobrem a organização do palco, os controles em janelas estreitas
  e a persistência das preferências.
- `tests/integration_room_controls.py`: janela FFplay gerada, dois publicadores
  Linux sintéticos e três cartões na grade. O teste verifica a prévia local,
  o painel de participantes, o palco, a tela cheia e o retorno à grade.
- Screenshots limitados à janela HyperCast do processo criado pelo teste.
