# Estatísticas na tela

Pressione **I** (fora de um campo de texto) ou use **Sala e interface → Estatísticas (I)** para mostrar ou esconder um painel pequeno no canto superior direito do palco. Ele aparece na janela normal e em tela cheia, e a escolha fica salva nas preferências. Se a faixa de miniaturas estiver no canto superior direito, o painel passa para o esquerdo.

Uma linha por vídeo, com um detalhe mais apagado embaixo:

| Linha | Mostra | Detalhe |
|---|---|---|
| ↑ Você | resolução codificada, FPS medido, codec e encoder, Mb/s codificados | alvo do upload, banda concedida pelo WebRTC, QP médio, limitação (CPU/rede) e pausa pelo servidor |
| ↓ pessoa / pessoa cam | resolução recebida, FPS, codec (`hw` quando o decoder informa eficiência energética), Mb/s recebidos | perda, jitter, espera média no jitter buffer, decode médio e travadas |
| ⇅ Rede | RTT do par ICE selecionado, caminho `direto`/`TURN` e transporte UDP/TCP/TLS | envio aparece separado só quando usa outro caminho |

O gráfico pequeno de cada linha mostra os Mb/s do último minuto. Detalhe em âmbar indica perda a partir de 2%, limitação do encoder ou envio pausado.

## Como é medido

- Com o painel aberto, o monitor de recepção lê as estatísticas WebRTC a cada 1 s em vez de 5 s, na mesma consulta. A recuperação automática e o log continuam avaliando a cada 5 s. Fechado, nada muda em relação à versão anterior.
- Mb/s, perda, buffer e decode são diferenças entre duas amostras. Na primeira amostra, ou quando um receptor é recriado, aparecem como `—` em vez de um pico falso.
- O envio usa os contadores do próprio encoder (os mesmos do relatório técnico). A limitação e o codec de reserva vêm do RTP de saída.
- Buffer + decode + RTT **não** somam a latência ponta a ponta (ADR 0004). Para isso, use a [validação visual](latency-validation.md).

## Privacidade

O painel é só local: não entra no relatório técnico nem no log. Mostra nomes que já aparecem no palco, nunca endereços IP, IDs de faixa ou credenciais, então pode ser compartilhado em print.

## Limites

- Disponível no Windows. O Linux ainda não tem o painel (o item de menu não aparece lá).
- Vídeos sem dono conhecido na sala não ganham linha.
- Sobre vídeo nativo, o painel recorta o vídeo atrás dele, como os outros controles do palco; não é translúcido.
- Observado em teste: logo depois que uma transmissão para e o último quadro fica congelado, o recorte pode continuar com o tamanho anterior (maior) do painel até o vídeo mudar.

Teste de fumaça: `HYPERCAST_SMOKE_NERD=1` abre o painel e imprime o conteúdo dele a cada 2 s (`GUI nerd: …`).
