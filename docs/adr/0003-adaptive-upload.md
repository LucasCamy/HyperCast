# ADR 0003 — Qualidade adaptativa

- Data: 2026-10-18
- Estado: Aceito; primeiro incremento de bitrate implementado
- Marco: incremento posterior à 0.9.0

## Contexto e decisão

Decisão: oferecer Automático e Manual; preservar os perfis 40/25/12 Mbps. No modo automático o perfil escolhido é teto, e feedback WebRTC continua sendo limite de congestionamento. Reduzir cedo sob congestionamento persistente e elevar gradualmente após janela estável, com histerese. Não ultrapassar teto nem gerar oscilação a cada amostra.

Começar por bitrate; alterações de FPS/resolução exigem capacidade de reconfiguração e ciclo de keyframe comprovados por backend. Falha na reconfiguração deve manter um estado seguro e permitir nova tentativa, sem afirmar que o novo valor foi aplicado. Compartilhar a política entre sistemas e manter aplicação nativa específica por encoder.

Aceite: simular banda variável e perda, respeitar teto e pausa de bitrate zero, testar recuperação sem oscilações; NVENC e AMF separados; explicitar limitações de hardware disponível.

## Primeiro incremento — 2026-10-20

Seletor Manual/Automático junto ao perfil de upload. A preferência é persistida; configurações anteriores sem esse campo continuam em Manual. Os dois modos respeitam o feedback de banda do WebRTC e o perfil como teto. Manual preserva a resposta direta ao orçamento informado; não significa bitrate fixo que ignore congestionamento.

Automático reduz imediatamente ao receber orçamento inferior ao alvo, com 10% de margem. Depois exige dez segundos de feedback estável e recente antes de elevar o alvo; os passos ocorrem no máximo a cada cinco segundos, limitados a 10% (mínimo 64 kbps, máximo 2 Mbps) e ao menor orçamento recente. Uma janela de dez segundos, agregada por segundo em até onze posições, evita elevar a qualidade por picos breves. Lacunas de feedback superiores a três segundos suspendem a subida. A política não cria estimativa de banda por timer.

Orçamento zero, ou inferior ao mínimo de 64 kbps do encoder, pausa o envio de vídeo; não converter esse valor em um alvo maior que a concessão WebRTC. A retomada no Automático começa em até 1 Mbps e solicita keyframe pelo caminho já existente. Este incremento não introduz redução autônoma de resolução/FPS nem estima congestionamento diretamente por perda/RTT: utiliza a decisão do controlador WebRTC existente.

Aplicação no Windows usa a reconfiguração existente e suas respostas de aceitação por NVENC/AMF. No Linux, consultar tipo, mutabilidade em execução e leitura da propriedade de bitrate; arredondar kbps para baixo. Rejeição pausa o envio e mantém sala/recepção disponíveis para nova tentativa. Leitura da propriedade Linux não comprova throughput nem sucesso interno do driver. Relatório separa alvo, configuração conhecida, fluxo medido e rejeição.

Ver [operação e testes](../adaptive-upload.md). Não há comprovação nova de latência fim a fim, congestionamento real em WAN ou sessão longa AMF/Bazzite; tais partes do aceite permanecem abertas.

## Correção 0.9.2 — cadência da fonte externa

O caminho Windows anterior reaplicava `SetRates.framerate_fps` ao encoder e ao pacer da captura. Como atualizações do WebRTC também refletem a cadência de entrada, uma perda temporária podia se tornar o novo limite da fonte e realimentar quedas subsequentes. Interface e CLI agora mantêm o FPS configurado pelo usuário, adaptando somente bitrate e pausa/retomada. O comportamento de captura/encode Linux não foi alterado. Essa decisão evita a realimentação; não garante FPS sustentado quando a GPU ou rede não comporta o alvo.
