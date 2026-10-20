# Relatório técnico — 0.9.0

Abra **Debug** e use **Copiar relatório técnico**. O conteúdo JSON pode ser enviado como texto para investigar uma sessão. Ele inclui versão/sistema, estado de conexão, contagem de reinícios de recepção, configuração solicitada, encoder/codec conhecidos, configuração aceita pelo backend quando disponível, quadros, FPS de envio/recepção e métricas RTP agregadas.

Somente campos de uma lista permitida são exportados. Nomes, IPs, URLs, títulos de salas, identificadores de participantes, status livre e credenciais não entram nesse relatório. Ele é diferente do botão legado de copiar o log da sessão. Não há envio automático de telemetria.

O RTT é da conexão WebRTC de recepção. A coleta ocorre a cada cinco segundos com timeout de três segundos, fora da pintura. Contadores RTP e a média de decode são acumulados nas faixas atualmente existentes, podendo reiniciar quando os receptores mudam. Campos indisponíveis ficam `null`; um relatório logo após conectar pode ainda não ter amostra de rede.

`send_encoded_mbps` mede unidades de acesso codificadas, sem overhead de transporte/áudio; `receive_sink_fps` soma os quadros que chegaram aos leitores das faixas visíveis. Não representa o FPS de cada monitor nem a taxa de apresentação física. `accepted_encoder_config` informa a configuração aceita/conhecida pelo backend; não é throughput medido. Captura, conversões e apresentação ainda não possuem tempos individuais nesse relatório.

A latência fim a fim permanece ausente: não pode ser deduzida do RTT ou da média de decode. Precisa de uma medição visual em bancada.

No incremento posterior à 0.9.0, `automatic_recovery` acrescenta os contadores `observed`, `mapped`, `recovering`, `exhausted`, `actions`, `reader_stalls`, `decoder_stalls` e `unmapped_samples`. As contagens de faixas refletem o estado/amostra atual; `mapped` conta associações utilizáveis na última amostra; os demais acumulam na sessão. Os motivos contam ações tomadas, não cada amostra sem progresso. Atualizar recepção manualmente repõe o orçamento por faixa e preserva os totais. `unmapped_samples` significa que o SDK não forneceu associação exata de uma amostra; o detector se abstém de recuperar essa faixa. Nenhum identificador é exportado.

O incremento de qualidade adaptativa acrescenta `adaptive_upload` (modo, teto, concessão WebRTC, alvo e contadores) e `encoder_rate_rejected`. O alvo é solicitado, não medido nem garantido pelo driver. O estado fica ausente quando não existe publicação local. Leitura de propriedade GStreamer não preenche automaticamente `accepted_encoder_config`.
