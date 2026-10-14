# Controles da sala · 0.7.1

O nome informado fica salvo por usuário. No Windows o arquivo é
`%LOCALAPPDATA%/HyperCast/display-name`; no Linux,
`$XDG_STATE_HOME/hypercast/display-name` ou `~/.local/state/hypercast/display-name`.
Editar o nome substitui o anterior ao sair do campo, entrar na sala ou fechar
normalmente o aplicativo. Credenciais e convites não fazem parte desse arquivo.
`HYPERCAST_NAME`, quando definido explicitamente, substitui o preenchimento sem
alterar o cache do usuário. Um cache inválido é ignorado.

**Recolher menu** esconde a lateral; **Abrir menu** a restaura. A largura liberada
é usada pelos vídeos. Seletores de origem abertos são fechados ao recolher.

No Windows, desmarque **Mostrar minha prévia** para interromper a geração da
textura de prévia e liberar seu apresentador DirectX. Captura, codificação e envio
continuam ativos. Reativar a opção produz novos quadros locais. A versão Linux
já não oferece prévia do próprio envio, portanto não faz esse processamento.

Nos participantes, o ponto verde e a indicação de transmissão identificam quem
compartilha mesmo fora do foco. A borda selecionada e **Em destaque** identificam
quem está sendo acompanhado. Recepção pausada não marca alguém como assistido.

Duplo clique no cartão do participante ou no vídeo mostra só aquela transmissão.
No Windows também oculta as miniaturas estáticas; a subscrição de destaque
mantém apenas o vídeo remoto escolhido. No Linux o destaque já usa uma única
transmissão. **Grid** restaura a visualização de múltiplas telas. Clique simples
continua selecionando a tela em destaque. O limite do servidor permanece três
publicadores ativos.

## Verificação

- Testes Rust: persistência, substituição e rejeição de cache inválido ou grande.
- `tests/integration_room_controls.py`: janela FFplay gerada, dois publicadores
  Linux sintéticos, sala isolada e três cartões no Grid. Testa ocultar/reativar a
  prévia sem encerrar a publicação, recolher/restaurar menu, duplo clique e retorno
  ao Grid. Tokens só em ambiente; a sala de teste é removida ao finalizar.
- Screenshots limitados à janela HyperCast do processo criado pelo teste.
