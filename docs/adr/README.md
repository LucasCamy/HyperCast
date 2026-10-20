# Decisões da 0.9.0

- [0001 — Recuperação e limites de falha](0001-recovery-and-failure-boundaries.md)
- [0002 — Recuperação automática da recepção](0002-video-reception-recovery.md)
- [0003 — Qualidade adaptativa](0003-adaptive-upload.md)
- [0004 — Diagnóstico e medição de latência](0004-diagnostics-and-latency.md)
- [0005 — Capacidades reais de GPU e matriz de compatibilidade](0005-hardware-capabilities.md)
- [0006 — Preferências persistentes e restauração segura](0006-persistent-preferences.md)

A 0.9.0 foi publicada com o escopo mínimo documentado abaixo. A ordem de implementação original foi: recuperação e limites de falha (0001); depois observabilidade (0004), recuperação de recepção (0002), adaptação (0003), capacidades (0005) e preferências (0006). Estados planejados não indicam funcionalidades já entregues.

Em 2026-10-19 foi definido um [escopo mínimo estável](../0.9.0.md): recuperação, relatório técnico seguro e preferências. Adaptação adicional, recuperação automática por faixa e novos backends permanecem incrementos futuros; não são bloqueadores dessa entrega reduzida.

Em 2026-10-19 começou o incremento posterior à versão estável: recuperação automática conservadora por publicação (0002), apoiada por contadores no relatório (0004). Os detalhes e limites estão nos respectivos ADRs e no [manual de recepção](../reception-recovery.md).

Em 2026-10-20 foi implementado o primeiro incremento de [qualidade adaptativa de bitrate](../adaptive-upload.md) (0003), mantendo as pendências físicas e de WAN explícitas.
