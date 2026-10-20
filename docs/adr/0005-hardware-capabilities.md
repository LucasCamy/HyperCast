# ADR 0005 — Capacidades reais de GPU e matriz de compatibilidade

- Data: 2026-10-18
- Estado: Aceito; primeiro incremento de capacidades implementado
- Marco: incremento posterior à 0.9.0

## Contexto e decisão

Decisão: consultar capacidades de codec, tamanho, FPS e reconfiguração do encoder na GPU da origem. Diferenciar capacidade de encode e decode e não decidir apenas pela marca. Desabilitar combinações comprovadamente incompatíveis com explicação; capacidades desconhecidas permitem tentativa com erro recuperável.

Fallback nunca escolhe software de alto consumo silenciosamente nem troca a GPU sem informar. Preservar pipelines Windows DirectX e Linux Portal/PipeWire separados, com políticas comuns testáveis.

Aceite: NVIDIA Windows, AMD Windows e Bazzite/NVIDIA em sessões longas, trocas de origem e queda de rede. AMF permanece sem validação física até existir hardware acessível; não marcar concluído por compilar.

## Primeiro incremento — 2026-10-20

Windows consulta HEVC/AV1 e limites de dimensões no dispositivo D3D11 do adaptador selecionado, por NVENC/AMF. A consulta não captura a tela. NVENC informa também suporte à reconfiguração de bitrate; AMF mantém essa informação desconhecida. Resultados desconhecidos permitem tentativa; somente incompatibilidades confirmadas desabilitam escolhas e o início da publicação, com explicação visível. O encoder efetivamente aberto valida novamente a resolução de envio antes da publicação.

Linux distingue presença do elemento GStreamer de capacidade real de uma sessão. Antes da inicialização do runtime, a disponibilidade é desconhecida. Depois de criar o elemento escolhido, consulta seus limites de entrada e mutabilidade da propriedade de bitrate. Não altera a ordem existente de preferência dos encoders nem introduz fallback para software. A identidade física da GPU da origem Portal/PipeWire ainda não é comprovada por essa consulta.

O modo Automático é recusado antes de publicar quando a impossibilidade de reconfigurar bitrate é conhecida. Dados ausentes não são convertidos em incompatibilidade. A consulta permanece fora da pintura no Windows; o cache tem até quatro adaptadores, validade de dois minutos e somente uma tarefa nativa em andamento. O encerramento da interface não aguarda essa tarefa.

O máximo de FPS permanece desconhecido neste incremento: limites de dimensões e aceitação de configuração não comprovam 120 FPS sustentados. Não inferir capacidade de decode ou desempenho a partir de encode. O relatório técnico exporta apenas campos escalares permitidos da publicação ativa.

Ver [limites e validação](../encoder-capabilities.md). AMF foi compilado, sem validação em GPU AMD física; Bazzite/RTX 5060 Ti, sessões longas e bancada de latência continuam pendentes. Este incremento não conclui todo o aceite do ADR.
