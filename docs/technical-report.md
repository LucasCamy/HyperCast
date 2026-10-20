# Relatório técnico — 0.9.1

Abra **Debug** e use **Copiar relatório técnico**. O conteúdo JSON pode ser enviado como texto para investigar uma sessão. Ele inclui versão/sistema, estado de conexão, contagem de reinícios de recepção, configuração solicitada, encoder/codec conhecidos, configuração aceita pelo backend quando disponível, quadros, FPS de envio/recepção e métricas RTP agregadas.

Somente campos de uma lista permitida são exportados. Nomes, IPs, URLs, títulos de salas, identificadores de participantes, status livre e credenciais não entram nesse relatório. Ele é diferente do botão legado de copiar o log da sessão. Não há envio automático de telemetria.

O RTT é da conexão WebRTC de recepção. A coleta ocorre a cada cinco segundos com timeout de três segundos, fora da pintura. Contadores RTP e a média de decode são acumulados nas faixas atualmente existentes, podendo reiniciar quando os receptores mudam. Campos indisponíveis ficam `null`; um relatório logo após conectar pode ainda não ter amostra de rede.

`send_encoded_mbps` mede unidades de acesso codificadas, sem overhead de transporte/áudio; `receive_sink_fps` soma os quadros que chegaram aos leitores das faixas visíveis. Não representa o FPS de cada monitor nem a taxa de apresentação física. `accepted_encoder_config` informa a configuração aceita/conhecida pelo backend; não é throughput medido. A seção `evidence` acrescenta os intervalos de CPU observáveis, conforme os limites abaixo.

A latência fim a fim permanece ausente: não pode ser deduzida do RTT ou da média de decode. Precisa de uma medição visual em bancada.

No incremento posterior à 0.9.0, `automatic_recovery` acrescenta os contadores `observed`, `mapped`, `recovering`, `exhausted`, `actions`, `reader_stalls`, `decoder_stalls` e `unmapped_samples`. As contagens de faixas refletem o estado/amostra atual; `mapped` conta associações utilizáveis na última amostra; os demais acumulam na sessão. Os motivos contam ações tomadas, não cada amostra sem progresso. Atualizar recepção manualmente repõe o orçamento por faixa e preserva os totais. `unmapped_samples` significa que o SDK não forneceu associação exata de uma amostra; o detector se abstém de recuperar essa faixa. Nenhum identificador é exportado.

O incremento de qualidade adaptativa acrescenta `adaptive_upload` (modo, teto, concessão WebRTC, alvo e contadores) e `encoder_rate_rejected`. O alvo é solicitado, não medido nem garantido pelo driver. O estado fica ausente quando não existe publicação local. Leitura de propriedade GStreamer não preenche automaticamente `accepted_encoder_config`.

`encode_capabilities` contém capacidades anunciadas do encoder da publicação local ativa, com origem `driver` no Windows e `gstreamer_element_caps` no Linux. Exporta suporte, dimensões mínimas/máximas, alinhamento vertical de superfície e reconfiguração de bitrate quando conhecidos. Valores ausentes/inválidos viram `null`; o máximo de FPS permanece desconhecido nos backends atuais. O alinhamento é de superfície, não uma exigência de que a altura visível tenha esse múltiplo. Não inclui identificadores de GPU, nomes privados ou conteúdo capturado. `decode_support` é `null` e `performance_validated` é `false`: esse registro não comprova decode, FPS sustentado ou latência. Consultas de pré-seleção em cache na interface não são exportadas como sessão ativa.

## Evidências por etapa

`evidence.stages` usa histogramas fixos de 32 posições por etapa. Exporta amostras, média, máximo, limites superiores aproximados de p50/p95 e idade da última amostra. São acumulados da sessão, agregando as origens/codecs usados e os vídeos apresentados; não são uma janela isolada por participante. O snapshot concorrente é aproximado. Etapas sem amostras ficam `null`, inclusive se a plataforma não as oferece. Uma amostra antiga não descreve necessariamente o comportamento atual.

| Campo | O que mede | O que não mede |
|---|---|---|
| `capture_acquire_call` | Chamada Windows para obter frame, incluindo espera/poll e eventuais retornos sem quadro | Tempo de criação do frame no compositor ou captura GPU isolada |
| `encode_call` | Chamada síncrona Windows do encoder, incluindo preparação/cópia, espera e retirada do pacote | Kernel de encode isolado ou tempo de rede |
| `encoded_packet_submit` | Entrega do pacote codificado à API local do publicador | Envio RTP efetivo ou confirmação de recepção |
| `gst_source_to_encoded_sink` | Linux: saída bruta da fonte até unidade codificada na entrada do appsink, com PTS convertido pelo segmento para running-time correspondente | Captura anterior ao PipeWire ou encode isolado |
| `gst_encoder_input_to_encoded_sink` | Linux: entrada do encoder até unidade codificada na entrada do appsink, incluindo parser/backpressure | Tempo puro da GPU |
| `render_prepare_cpu` | Linux: conversão para RGBA e preparação/atualização da textura egui | Upload concluído na GPU, apresentação ou scanout |
| `present_call` | Windows: chamada DirectX de apresentação, incluindo processamento e fence local | Aparição física dos pixels no monitor |
| `reader_to_render_start` | Entrega do frame pelo leitor ao estado da interface até iniciar preparação/apresentação | Jitter, decode anterior ao leitor ou latência fim a fim |

No Linux, a correlação mantém até 64 timestamps por fronteira, sem reter frames. Só associa o running-time exato e não ambíguo calculado pelo SEGMENT de cada fronteira; a compensação declarada pelo GstVideoEncoder é respeitada, sem inferir offsets pela ordem dos quadros. Descarte, PTS reescrito pelo encoder/parser ou ausência de timestamp incrementam `correlation_misses` quando uma saída não pode ser associada. A etapa fica desconhecida se nenhuma associação for possível. Não escolher outro quadro para produzir uma medida. `gstreamer_pipeline_frames` acrescenta contadores acumulados da captura atual (fonte, entrada do encoder, saída codificada e entrega ao consumidor); fica ausente sem captura ativa.

O decode continua vindo das estatísticas WebRTC em `network.video_rtp.average_decode_ms`, média cumulativa dos receptores atuais quando informada pelo SDK. Não há percentis por frame nem medida separada do kernel do decoder. `last_encode_window` expõe, no Windows, a última janela nominal de cinco segundos: chamadas, bytes, keyframes, maior pacote e QP médio/amostras quando disponíveis. QP ausente fica `null`; QP de codecs diferentes não deve ser comparado como qualidade equivalente. A última janela pode ser menor na inicialização e não mede pacotes RTP entregues.

`evidence.events` guarda até 64 transições enumeradas: conexão/retry, reconexão, publicação, troca de origem, pausa, rejeição de bitrate, alvo de bitrate e recuperação de recepção. Flags são observadas no máximo a cada 250 ms; transições mais curtas podem não aparecer. Usa somente tempo relativo de sessão e números limitados, sem status livre, nomes, IDs, endereço, título de janela ou credenciais. Não contém stack trace. Sair/recriar sessão pode descartar o histórico; copie o relatório antes de sair ou fechar. Sem telemetria automática.

## Como investigar

- Confira backend, codec, resolução, perfil e alvo de bitrate junto de `send_encoded_mbps`; redução de bitrate pode reduzir a qualidade mesmo sem aumento no tempo de encode.
- Compare as fronteiras GStreamer, considerando FPS configurado e descarte intencional da fila bruta. Diferenças entre contadores ajudam a localizar perdas de trabalho, mas não provam sozinhas um defeito.
- Tempo alto em `reader_to_render_start` ou `render_prepare_cpu` aponta espera/preparação local a investigar. Quadros retidos/reapresentados após resize também podem ter idade alta. Eventos ajudam a relacionar o momento com reconexão ou recuperação.

Microbenchmark local de cinco medições mais correlação com locks, 100 mil ciclos sintéticos em build de teste: média aproximada de 0,48 µs/ciclo no Windows e 0,79 µs/ciclo no Linux/WSL. É custo médio da instrumentação nesse host, não pior caso, consumo total do app ou garantia em AMD/Bazzite. Não há logging por quadro. Buffers e histórico são limitados; exportação aloca somente ao ser solicitada. Execuções smoke opt-in também escrevem o relatório sanitizado no log de teste ao encerrar.

Para comprovar a latência visual, use o [procedimento de bancada](latency-validation.md) e a referência local incluída no pacote. `end_to_end_latency_ms` continua `null` até existir uma medição real; não preencher com RTT ou soma dos intervalos acima.
