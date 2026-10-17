# Resoluções ultrawide · 0.7.2

O limite fixo de 3840×2160 foi removido da captura de janela, encoders e
recepção Windows/Linux. As origens podem ter até 8192 pixels por eixo, com
limite total de 33.554.432 pixels. Esse orçamento é verificado também antes de
alocar quadros recebidos, independentemente das informações de publicação.

A resolução nativa é a primeira opção. NVENC consulta `NV_ENC_CAPS_WIDTH_MAX`
e `NV_ENC_CAPS_HEIGHT_MAX` para o codec da GPU escolhida; AMF consulta as faixas
das capacidades de entrada do componente. Se a origem exceder essas faixas,
o encoder recebe uma imagem reduzida, com a mesma proporção, dentro do limite.

Dimensões ímpares são alinhadas para números pares sem descartar bordas.
No Windows, o alinhamento nativo replica a última coluna/linha, preservando
os pixels originais; a filtragem é usada quando há redução real de tamanho. Uma
origem **4587×1920** resulta em **4588×1920** quando suportada. Um encoder
limitado a 3840×2160 recebe aproximadamente **3840×1608**. Os arredondamentos
de até um pixel podem alterar minimamente a proporção.

No Windows a conversão e o redimensionamento usam shader DirectX, sem copiar
pixels para RAM. O encoder permanece com uma superfície privada e uma cerca
GPU antes de liberar a captura. Mudanças de tamanho recriam o encoder e
atualizam a resolução anunciada com novo quadro-chave. O status do envio
mostra a dimensão efetiva; o diagnóstico registra origem e saída.

No Linux as capacidades negociadas do plugin determinam o tamanho aceito;
`videoconvertscale` ajusta as dimensões antes do encoder. O caminho portátil
continua permitindo conversões por CPU. A partir da 0.8.11, dimensões pares são
negociadas antes de iniciar a origem, sem forçar uma renegociação no callback do
PipeWire. Uma origem ímpar pode ser reamostrada para o par mais próximo: o teste
de 4587×1920 negociou 4586×1920, preservando a imagem inteira. O alinhamento por
replicação de borda descrito acima é específico do Windows.
A documentação do elemento explica
sua negociação de tamanho e proporção:
https://gstreamer.freedesktop.org/documentation/videoconvertscale/videoconvertscale.html

**Todos os participantes que forem receber essas resoluções precisam atualizar
para 0.7.2 ou posterior**. Os clientes anteriores ainda rejeitam quadros maiores.
Nenhuma alteração de configuração do SFU é necessária. O teto de bitrate segue
40 Mbps no envio inicial, adaptado pelo WebRTC, com três publicadores por sala.

## Validação

`generated_ultrawide_gpu_lifecycle` gera texturas GPU de 4587×1920 e testa
HEVC/AV1 em três ciclos de inicialização, codificação e encerramento. Também
verifica falha por dimensão acima do orçamento e textura incompatível;
os contadores de proprietários nativos retornam à linha de base.

O teste Linux `--linux-media-test`, com `HYPERCAST_LINUX_TEST_SIZE=4587x1920`,
repete captura sintética, codificação HEVC, decodificação e liberação do contexto,
incluindo pacote malformado. `tests/integration_linux.py` aceita essa dimensão
sintética para validar publicação pelo SFU, recepção Windows D3D11, áudio e GUI
Linux. Nenhum desses testes captura o desktop pessoal.

Aceitar a resolução não garante 120 FPS sustentados no monitor do colega;
a taxa real depende da GPU, codec, captura e renderização. Essa validação física
continua necessária, assim como a medição de latência fim a fim.
