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

Um clique no cartão do participante ou no próprio vídeo coloca aquela transmissão
em destaque. No Windows, um duplo clique no destaque também oculta as miniaturas.
No Linux, o destaque já mostra uma única transmissão. O ícone de **Grade** volta
à visualização de múltiplas telas. O limite do servidor permanece três publicadores ativos.

## Verificação

- Testes Rust: persistência, substituição e rejeição de cache inválido ou grande.
- `tests/integration_room_controls.py`: janela FFplay gerada, dois publicadores
  Linux sintéticos, sala isolada e três cartões no Grid. Testa ocultar/reativar a
  prévia sem encerrar a publicação, recolher/restaurar menu, duplo clique e retorno
  ao Grid. Tokens só em ambiente; a sala de teste é removida ao finalizar.
- Screenshots limitados à janela HyperCast do processo criado pelo teste.
