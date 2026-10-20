# Capacidades de encode — incremento posterior à 0.9.0

## Comportamento

O seletor de formato e o início do compartilhamento bloqueiam apenas incompatibilidades conhecidas. Uma explicação aparece no painel; dado desconhecido permite tentar novamente. Não existe escolha silenciosa de outra GPU, codec ou encoder de software. As preferências anteriores continuam válidas, inclusive o modo Manual como padrão.

No Windows, a consulta abre um dispositivo D3D11 no adaptador selecionado e uma sessão de consulta NVENC ou AMF, sem obter frames do desktop. Verifica HEVC/AV1 e dimensões anunciadas. NVENC consulta também reconfiguração dinâmica de bitrate; AMF deixa esse campo desconhecido. Exceção/erro de consulta resulta em desconhecido, sem encerrar o app. A publicação usa as capacidades do encoder realmente aberto e verifica a resolução solicitada, já considerando o limite de envio escolhido. Janelas de aplicativos só têm dimensões conhecidas após a captura inicial.

A interface tem uma tarefa de consulta em andamento e cache de até quatro adaptadores, válido por 120 segundos. Resultados de outro adaptador não são aplicados à seleção atual. Dispositivo, sessão e DLL são liberados no mesmo worker por RAII. A interface não aguarda o worker ao fechar. Isso não isola falhas nativas do driver em outro processo: acesso inválido ou chamada nativa que nunca retorna não é um erro Rust recuperável.

No Linux, ausência de plugin/elemento pode impedir uma escolha; antes da inicialização GStreamer a disponibilidade é desconhecida. O elemento efetivamente criado informa um envelope de dimensões aceitas e a mutabilidade da propriedade de bitrate. Alternativas com limites ausentes tornam a dimensão desconhecida. A validação do tamanho não modifica caps dentro do callback PipeWire. Presença no registro e caps do elemento não comprovam suporte do driver ou identidade da GPU da origem. A seleção e os caminhos de memória existentes são preservados.

Quando a ausência de reconfiguração é conhecida, Automático fica indisponível no seletor Windows ou retorna erro recuperável antes da publicação Linux. A sala e a recepção continuam disponíveis. Falha de consulta permite tentativa; a resposta real da reconfiguração continua sendo verificada pelo controlador de bitrate.

## Limites e aceite aberto

O campo máximo de FPS permanece desconhecido. Não transformar throughput teórico de blocos em promessa de FPS. Inicialização aceita e limites de resolução não comprovam 120 FPS, funcionamento contínuo ou latência abaixo de 100 ms. Capacidade de decode não é inferida de encode.

AMF precisa de validação física AMD. Bazzite/RTX 5060 Ti precisa de execução no computador do usuário com Portal/PipeWire real. WSL e texturas sintéticas cobrem regressões, mas não substituem esses testes nem sessões prolongadas com queda de rede. O relatório técnico separa capacidades anunciadas de fluxo medido; ver [campos exportados](technical-report.md).

## Validação desta implementação

- 45 testes unitários Windows e 42 Linux passaram; cobrem limites conhecidos, desconhecidos, dados inválidos e estados de interface. A execução serial Linux detectou consulta prematura ao GStreamer; o acesso ao registro foi protegido antes da inicialização.
- Consulta NVENC repetida no adaptador local, sem captura: enumeração HEVC/AV1, limites, adaptador inválido e retorno do contador de sessões ao valor inicial.
- Texturas sintéticas NVENC HEVC/AV1: reconfiguração de bitrate sem recriar encoder; limites de saída e FPS solicitados.
- Pipeline Linux sintético e publicação em sala isolada: seis etapas de troca de origem/codec passaram, preservando áudio/recepção. Não usar conteúdo de tela ou áudio pessoal nesses testes.

Não foi produzido novo instalador ou release por este incremento.
