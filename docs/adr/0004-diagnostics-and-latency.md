# ADR 0004 — Diagnóstico e medição de latência

- Data: 2026-10-18
- Estado: Aceito; planejado
- Marco: 0.9.0

## Contexto e decisão

Decisão: observações por etapa: captura, encode, envio, RTP recebido, decode e apresentação. Mostrar valores solicitados e aplicados, codec/backend, FPS, bitrate, perda, RTT, motivo e contagem de recuperação. Usar amostras agregadas e buffers limitados; evitar logging por quadro no caminho crítico.

Exportar relatório com versão/esquema, sem JWT, chave compartilhada, cabeçalho de autorização ou metadados privados. Exportação explícita do usuário; não enviar telemetria automaticamente.

RTT não mede latência fim a fim. Comprovar sub-100 ms exige bancada com referência visual/relógio e método documentado; não inferir isso da soma de métricas incompletas.

Aceite: teste de sanitização com credenciais sintéticas, buffers limitados, overhead medido e relatório útil Windows/Linux.
