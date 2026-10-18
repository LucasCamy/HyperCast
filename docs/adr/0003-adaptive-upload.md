# ADR 0003 — Qualidade adaptativa

- Data: 2026-10-18
- Estado: Aceito; planejado
- Marco: 0.9.0

## Contexto e decisão

Decisão: oferecer Automático e Manual; preservar os perfis 40/25/12 Mbps. No modo automático o perfil escolhido é teto, e feedback WebRTC continua sendo limite de congestionamento. Reduzir cedo sob congestionamento persistente e elevar gradualmente após janela estável, com histerese. Não ultrapassar teto nem gerar oscilação a cada amostra.

Começar por bitrate; alterações de FPS/resolução exigem capacidade de reconfiguração e ciclo de keyframe comprovados por backend. Falha na reconfiguração deve manter um estado seguro e permitir nova tentativa, sem afirmar que o novo valor foi aplicado. Compartilhar a política entre sistemas e manter aplicação nativa específica por encoder.

Aceite: simular banda variável e perda, respeitar teto e pausa de bitrate zero, testar recuperação sem oscilações; NVENC e AMF separados; explicitar limitações de hardware disponível.
