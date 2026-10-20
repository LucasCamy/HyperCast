# ADR 0004 — Diagnóstico e medição de latência

- Data: 2026-10-18
- Estado: Aceito; instrumentação de fronteiras CPU implementada; medição física pendente
- Marco: 0.9.0

## Contexto e decisão

Decisão: observações por etapa: captura, encode, envio, RTP recebido, decode e apresentação. Mostrar valores solicitados e aplicados, codec/backend, FPS, bitrate, perda, RTT, motivo e contagem de recuperação. Usar amostras agregadas e buffers limitados; evitar logging por quadro no caminho crítico.

Exportar relatório com versão/esquema, sem JWT, chave compartilhada, cabeçalho de autorização ou metadados privados. Exportação explícita do usuário; não enviar telemetria automaticamente.

RTT não mede latência fim a fim. Comprovar sub-100 ms exige bancada com referência visual/relógio e método documentado; não inferir isso da soma de métricas incompletas.

Aceite: teste de sanitização com credenciais sintéticas, buffers limitados, overhead medido e relatório útil Windows/Linux.

## Entrega mínima da 0.9.0

[Relatório técnico](../technical-report.md) com lista permitida, contadores/FPS, RTT e configuração conhecida pelo backend. Tempos individuais de captura, encode e apresentação, histórico de eventos estruturado e medição visual fim a fim permanecem fora deste incremento.

## Incremento de recuperação da recepção

O relatório inclui `automatic_recovery`: faixas observadas, recuperando, limite de tentativas, total de ações, motivos leitor/decoder e amostras sem associação exata. São contadores agregados. IDs, nomes, conteúdo de tela e credenciais permanecem fora da exportação. A coleta é compartilhada com o observador, evitando uma segunda consulta concorrente de estatísticas.

## Incremento 0.9.1 — 2026-10-20

Histogramas de tamanho fixo por etapa observável e histórico limitado de transições enumeradas, compartilhados por worker e interface sem incluir frames ou strings privadas. Instrumentação Windows cobre chamadas de captura/encode/envio/apresentação; Linux correlaciona fonte/entrada do encoder à saída codificada por PTS convertido em running-time através do SEGMENT de cada pad, com associação exata e mede preparação da textura. Timestamps ausentes ou ambíguos não recebem substitutos estimados. Decode usa a média cumulativa disponibilizada pelo SDK. Relatório inclui QP/tamanho de quadros quando fornecidos, contadores das fronteiras GStreamer e limites explícitos.

Microbenchmark local mede custo médio da instrumentação, sem extrapolar para máquinas não testadas. Rotas de mídia, codecs e renderização mantêm seu comportamento; novos probes GStreamer apenas observam buffers e não reconfiguram caps. Testes verificam limites de memória, ausência de campos privados e recusa de correlação ambígua.

A implementação não obtém tempos internos de kernels GPU, instante físico de apresentação ou captura anterior ao PipeWire. A [validação visual](../latency-validation.md) usa referência local, câmera e comparação do mesmo conteúdo nos dois monitores, sem depender de relógios dos PCs sincronizados. A referência e o método acompanham os pacotes para coleta posterior. Comprovação sub-100 ms e matriz física AMD/Bazzite continuam pendentes. Ver [campos e interpretação](../technical-report.md).
