# Controles da sala · 0.7.1

O nome informado fica salvo por usuário. No Windows o arquivo é
`%LOCALAPPDATA%/HyperCast/display-name`; no Linux,
`$XDG_STATE_HOME/hypercast/display-name` ou `~/.local/state/hypercast/display-name`.
Editar o nome substitui o anterior ao sair do campo, entrar na sala ou fechar
normalmente o aplicativo. Credenciais e convites não fazem parte desse arquivo.
`HYPERCAST_NAME`, quando definido explicitamente, substitui o preenchimento sem
alterar o cache do usuário. Um cache inválido é ignorado.

**Participantes**, à esquerda do nome da sala, recolhe ou restaura o painel lateral.
Em janelas estreitas, aparece como um ícone de painel. Os ícones de **Grade** e
**Tela cheia** ficam à direita na linha do título.

Na barra inferior, **Compartilhar tela** e **Ligar câmera** ficam agrupados.
O cabeçalho informa quando a tela ou a câmera está ativa.
Os ícones de pausa e reconexão da recepção ficam ao lado dos controles de
compartilhamento ou à esquerda na segunda linha em janelas estreitas.
**Mais opções** e **Sair da sala** ficam à direita ou em uma segunda linha em
janelas estreitas. Use **Mais opções → Trocar de sala** para abrir a lista de
salas.
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

No Windows, clique no **olho** ao lado da engrenagem para interromper a geração da
textura de prévia e liberar seu apresentador DirectX. Captura, codificação e envio
continuam ativos. Reativar a opção produz novos quadros locais. A versão Linux
já não oferece prévia do próprio envio, portanto não faz esse processamento.

O alto-falante e o controle de volume no cabeçalho ajustam o som recebido.
O seletor de compartilhamento mantém a recepção de vídeo e áudio ativa.

Nos participantes, o ponto verde e a indicação de transmissão identificam quem
compartilha mesmo fora do foco. A borda selecionada e **Em destaque** identificam
quem está sendo acompanhado. Recepção pausada não marca alguém como assistido.

Um clique no cartão do participante coloca a tela dele (ou a câmera, se ele não
compartilha tela) no palco. No Linux, o destaque mostra uma única transmissão e o
ícone de **Grade** volta à visualização de múltiplas telas. O limite do servidor
permanece três publicadores ativos.

## Palco e faixa de vídeos (Windows)

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
- Quando a tela do palco para, a câmera de quem compartilhava assume o palco.
- Se o palco ficar menor que 480 × 270, a faixa vira uma pílula que lista os
  vídeos para escolher o do palco.
- Clique duas vezes no palco para entrar ou sair da tela cheia.

## Controles sobre o vídeo (Windows)

Os controles ficam sobre a transmissão e aparecem quando o mouse se mexe. Somem
2 s depois, exceto enquanto o mouse está sobre eles, um menu está aberto ou a
faixa está sendo arrastada. Em tela cheia o cursor também some.

- No canto superior esquerdo de cada vídeo do palco aparece o nome de quem
  transmite.
- A barra inferior reúne **Tudo**, **Telas** e **Câmeras** (o que você quer
  ver); **Palco** ou **Grade**; **Lado a lado** quando houver; som com volume e
  porcentagem; **Reconectar recepção**; e **Tela cheia**.
- Em palcos estreitos, as escolhas de visualização ficam num menu e o volume num
  popover do alto-falante.
- A faixa sobe para não ficar embaixo da barra enquanto ela aparece.

Os controles são recortes opacos no vídeo, porque o vídeo é desenhado em
janelas nativas acima da interface. O cabeçalho da sala mostra só o nome, o
estado e **Participantes**. Em tela cheia não há barra fixa. Atalhos: **Esc**
ou **F11** saem, **M** silencia e **F5** reconecta a recepção.

O que fica oculto deixa de ser recebido. Em **Câmeras**, o áudio das telas
continua. O **olho** da sua prévia continua decidindo apenas se a sua tela e a
sua câmera aparecem para você. Modo, filtro e encaixe da faixa ficam salvos nas
preferências. Perfis anteriores à 0.9.7 abrem uma vez no palco em vez da Grade.

`HYPERCAST_TRACE_STAGE=1` imprime, a cada 2 s, o modo, o vídeo do palco, os
cartões, as assinaturas pedidas e os quadros apresentados por vídeo. Use só em
validação local.

## Verificação

- Testes Rust: persistência, substituição e rejeição de cache inválido ou grande.
- `tests/integration_room_controls.py`: janela FFplay gerada, dois publicadores
  Linux sintéticos, sala isolada e três cartões no Grid. Testa ocultar/reativar a
  prévia sem encerrar a publicação, recolher/restaurar menu, duplo clique e retorno
  ao Grid. Tokens só em ambiente; a sala de teste é removida ao finalizar.
- Screenshots limitados à janela HyperCast do processo criado pelo teste.
