# Controle de qualidade do envio

Incremento de desenvolvimento posterior à 0.9.0. Em **Formato de vídeo**, selecione o perfil de upload (40/25/12 Mbps) e o controle **Manual** ou **Automático**. Pare de compartilhar antes de mudar. A escolha fica salva e vale para monitores e janelas. Preferências antigas usam Manual.

Ambos os modos respeitam a banda concedida pelo WebRTC. Manual segue esse limite diretamente, limitado ao perfil. Automático reduz com margem quando a concessão cai e recupera gradualmente após estabilidade. Não aumenta o consumo acima do perfil escolhido nem usa software silenciosamente. Esse modo não reduz automaticamente o teto em uma rede saudável, nem analisa o conteúdo da tela para economizar banda.

## Política

- Queda: alvo até 90% da concessão, respeitando o mínimo de 64 kbps e o teto do perfil.
- Concessão abaixo de 64 kbps, inclusive zero: envio de vídeo pausado.
- Retomada após pausa: até 1 Mbps, limitada à concessão atual.
- Subida: dez segundos de feedback recente e estável; passo de até 10%, com mínimo de 64 kbps e máximo de 2 Mbps, no máximo uma vez a cada cinco segundos. O menor orçamento recente também limita o aumento.
- Feedback com lacuna superior a três segundos: suspender a subida e começar uma nova janela de estabilidade.
- A janela usa no máximo onze amostras agregadas por segundo. A política comum não consulta rede, disco ou driver por quadro.

A pausa limita o envio. No Linux, o pipeline de captura/encode continua ativo e os pacotes de vídeo são descartados durante a pausa; não há promessa de suspender todo o custo da GPU. Áudio e recepção seguem suas políticas existentes. Resolução/FPS não são alterados por esta política adicional; os limites/configurações já existentes dos backends e SDK continuam válidos.

## Reconfiguração e diagnóstico

Windows aplica o alvo pelas rotinas existentes de NVENC/AMF. Uma recusa do orçamento mantém o vídeo pausado; um erro que invalide o encoder permite parar/tentar compartilhar novamente, sem encerrar a aplicação. Linux verifica propriedade u32, mutabilidade em execução e leitura após escrever o valor em kbps. Uma recusa mantém a sala e informa a necessidade de tentar novamente.

**Debug → Copiar relatório técnico** inclui `adaptive_upload`: modo, teto, concessão WebRTC, alvo e contadores de redução/aumento. `encoder_rate_rejected` distingue recusa do encoder de uma simples pausa de banda. O alvo não é bitrate medido; consulte `send_encoded_mbps` para throughput de vídeo, sem áudio/overhead. A leitura de configuração Linux não comprova o bitrate real produzido pelo driver.

## Validação

Testes de política usam concessões controladas para queda, recuperação, alternância de banda, feedback esparso, pausa/retomada e limites numéricos. Não representam perda injetada numa WAN. Preferências verificam persistência, migração do campo ausente para Manual e rejeição de valores inválidos.

Os casos ignorados `adaptive_bitrate_reconfigures_nvenc_without_recreating_encoder` e `adaptive_bitrate_reconfigures_gstreamer_encoders` usam imagens sintéticas e relógio virtual para exercitar orçamento variável, aceitação e continuidade da codificação. O Windows verifica também o número de encoders vivos. O Linux verifica a propriedade e novos pacotes com NVENC HEVC/AV1 e x265 HEVC. Não capturam conteúdo pessoal.

`publisher_source_and_codec_switches` exerce seis etapas de origem/codec com publicação e recepção reais em sala isolada, agora em Automático. O executor existente `tests.integration_linux_publisher_switch` aceita os novos filtros em `HYPERCAST_SWITCH_CASE`.

AMF é compilado e usa a política comum, mas segue sem teste físico. A sessão prolongada no Bazzite e a resposta a perda/congestionamento real na internet continuam pendentes. Este incremento não mede latência fim a fim e ainda não cria uma release.

## Resultado em 2026-10-20

- 42 testes unitários no Windows e 38 no Linux passaram.
- NVENC Windows: HEVC/AV1 continuaram codificando após reduções e subidas de bitrate; configuração conhecida igualou o alvo; contador de encoders vivos permaneceu constante e voltou à base ao encerrar.
- GStreamer Linux: NVENC HEVC/AV1 e x265 HEVC aceitaram a reconfiguração em execução, com leitura da propriedade e continuidade de pacotes.
- Publicação/recepção Linux em sala isolada passou por seis etapas de origem/codec com Automático ativo, incluindo erro recuperável na tarefa de captura e parar/retomar. Isso verifica integração com feedback real do SDK; não injeta congestionamento na WAN.
- A UI Windows restaurou Automático em perfil isolado, permitiu mudar para Manual, salvou a escolha e reabriu em Manual, preservando 1080p, 60 FPS, 12 Mbps e volume de 35%. Foi verificada a apresentação no painel rolável, sem conectar à sala nem capturar conteúdo pessoal.

Os casos que exigem GPU/sala foram executados separadamente dos unitários. O artefato Windows em modo de desenvolvimento também compilou. AMF e Bazzite físico continuam pendentes, assim como empacotamento e nova release.
