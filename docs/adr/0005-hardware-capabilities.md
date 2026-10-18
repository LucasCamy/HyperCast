# ADR 0005 — Capacidades reais de GPU e matriz de compatibilidade

- Data: 2026-10-18
- Estado: Aceito; planejado
- Marco: 0.9.0

## Contexto e decisão

Decisão: consultar capacidades de codec, tamanho, FPS e reconfiguração do encoder na GPU da origem. Diferenciar capacidade de encode e decode e não decidir apenas pela marca. Desabilitar combinações comprovadamente incompatíveis com explicação; capacidades desconhecidas permitem tentativa com erro recuperável.

Fallback nunca escolhe software de alto consumo silenciosamente nem troca a GPU sem informar. Preservar pipelines Windows DirectX e Linux Portal/PipeWire separados, com políticas comuns testáveis.

Aceite: NVIDIA Windows, AMD Windows e Bazzite/NVIDIA em sessões longas, trocas de origem e queda de rede. AMF permanece sem validação física até existir hardware acessível; não marcar concluído por compilar.
