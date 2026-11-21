# Vídeo pausado pelo SFU

Sintoma relatado: após muito tempo na sala, uma transmissão que começa fica
parada só para quem assiste. **Reconectar recepção** não resolvia; sair e
entrar resolvia.

## Causa

`congestion_control.allow_pause: true` permitia ao alocador do LiveKit pausar
uma faixa quando a estimativa de banda de descida do espectador não comporta
todas. As telas do HyperCast têm uma camada só (HEVC/AV1, 12–40 Mbps); pausar
é a única redução disponível. Uma conexão longa acomoda a estimativa ao que já
recebe; uma tela nova não cabe e é pausada. Uma entrada nova parte de uma
estimativa nova, por isso funcionava.

O SDK Rust 0.9.4 descartava `StreamStateUpdate`: o app não via a pausa. Sem
RTP, a recuperação automática trata a faixa como tela estática. O antigo
**Reconectar recepção** só desabilitava e reabilitava as faixas na mesma
conexão, mantendo a estimativa. Um log local de 2026-11-19 mostra o padrão:
pacotes recebidos de uma tela congelados por cerca de 35 s, com ~4% de perda
prévia, enquanto a outra tela seguia.

## Mudanças

- **SFU:** `allow_pause: false` em `livekit.yaml` e `livekit.dev.yaml`. Com banda
  insuficiente o vídeo perde qualidade ou engasga, e se recupera sozinho, em vez
  de ficar pausado. Em produção, copie `tools/deploy_allow_pause_on_node.py`
  para `/etc/hypercast` no nó e rode como root com as salas vazias. Ele altera
  só essa linha no ConfigMap e no Secret de configuração TURN, guarda um
  snapshot para rollback, reinicia o SFU e restaura a configuração anterior se
  o rollout falhar. Reiniciar a única réplica encerra as salas; `--disconnect-active`
  pula a verificação de salas vazias. Aplicado em produção em 2026-11-21 com
  essa flag (1 participante reconectou sozinho; snapshot em
  `/etc/hypercast/allow-pause-rollback.json`).
- **SDK:** `StreamStateUpdate` vira `RoomEvent::TrackStreamStateChanged`, emitido
  só em transições, e atualiza `stream_state()` da faixa.
- **SDK:** após um reinício completo da sessão, as assinaturas desejadas são
  reenviadas na sessão nova. Antes, os pedidos feitos durante o reinício iam para
  a sessão que estava fechando e eram descartados; com `auto_subscribe` desligado,
  o espectador não voltava a receber. Isso também afetava reconexões completas
  causadas pela rede.
- **App (Windows e Linux):** **Reconectar recepção**/F5 refaz a sessão RTC dentro
  da mesma sala (novas PeerConnections, estimativa de banda nova), como sair e
  entrar, mantendo o último quadro na tela. Quem está publicando mantém o
  reinício só de recepção, que não interrompe a própria tela, a menos que o SFU
  tenha de fato pausado algum vídeo; nesse caso a sessão é refeita e o SDK
  republica as faixas locais.
- Vídeo pausado pelo SFU aparece no cartão como "Sem progresso - use Atualizar
  recepção". O diagnóstico registra `SFU paused a subscribed video` e a retomada.

## Validação em 2026-11-21

Windows, SFU local, sala isolada, janela FFplay gerada, release:

- Espectador apertou F5: sessão refeita, quadros novos em ~300 ms, sem
  republicação do publicador. Repetido uma segunda vez com o mesmo resultado.
- Publicador apertou F5: caminho só de recepção, publicação intacta e
  espectador continuou recebendo.
- Antes da correção de reassinatura, o mesmo teste mostrou o espectador parado
  após o reinício (`receivers=0`).

Não verificado: a pausa por congestionamento em si (exige congestionar a
descida de verdade) e o fluxo no Linux além da compilação.
