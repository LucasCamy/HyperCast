# ADR 0004 — Diagnóstico e medição de latência

- Data: 2026-10-18
- Estado: Aceito; relatório mínimo implementado; instrumentação completa planejada
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
