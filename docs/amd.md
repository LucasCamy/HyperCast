# GPUs AMD — 0.6.1

A publicação agora escolhe o encoder pelo dispositivo da captura: NVENC em
NVIDIA, AMF em AMD. Isso vale para monitor (DXGI) e janela (WGC). Não é preciso
instalar um codec separado; o runtime amfrt64.dll é fornecido pelo driver AMD
Adrenalin e carregado somente de Windows/System32. Sem cópia de pixels para RAM:
textura BGRA privada, conversão GPU para NV12, AMF e somente bitstream na CPU.
Capturas FP16/scRGB são convertidas para SDR na GPU, como no caminho NVIDIA.

## RX 5700 XT

Use **HEVC / H.265**. A RX 5700 XT (Navi10, VCN 2.0) não possui codificação
nem decodificação AV1 por hardware. Os amigos que compartilham para esse PC
também devem selecionar HEVC, mesmo usando RTX mais nova. Trocar driver não
adiciona AV1 a esse hardware. Matriz oficial:
https://github.com/GPUOpen-LibrariesAndSDKs/AMF/wiki/GPU-and-APU-HW-Features-and-Support.

O usuário informou Adrenalin 26.8.1. Ainda falta validação nesta RX 5700 XT;
não foi necessário trocar o servidor, SFU, TURN ou os convites.
Se faltar runtime AMF, reinstale o driver AMD. Erros mostram etapa e status
AMF no diagnóstico; o encoder deve ser reconstruído após erro.

## Pipeline e limites da validação

Uso ultra low latency, preset speed, CBR, GOP inicial de um segundo, headers
nos keyframes, sem pre-analysis, VBV inicial de um quadro e uma submissão por vez.
O HEVC AMF usa quadros I/P; o modo AV1 ULL fica condicionado ao hardware/driver.
Timeouts de cópia e QueryOutput são limitados a um segundo; não há fila ilimitada.
Texturas pertencem ao encoder até concluir a codificação síncrona.

Teste real na Radeon integrada do Ryzen 7 9700X, driver Windows 32.0.21030.2001:
120 quadros sintéticos HEVC 1280x720, quatro keyframes solicitados, reconfiguração
de bitrate/FPS, repetição de quadro, prévia GPU e decodificação D3D11VA. Nenhum
conteúdo de usuário é capturado. Cerca de 31 ms por quadro nesse teste síncrono;
isso não demonstra 120 FPS. A integração de uma janela sintética WGC na AMD também publicou HEVC pelo SFU
e apresentou os quadros no cliente desktop via D3D11VA. Ainda precisamos medir movimento, QHD e latência
fim a fim na RX 5700 XT. AV1 AMD não foi validado em GPU que suporte codificá-lo.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tests/integration_amf.ps1
$env:HYPERCAST_TEST_AMD_ADAPTER="1" # índice DXGI mostrado pelo probe
python -m tests.integration_amf_window
```

O teste exige ferramenta C++/SDK Windows no ambiente de desenvolvimento;
o instalador final não exige essas ferramentas. O arquivo sintético de teste
é removido ao final. O aplicativo preserva o caminho D3D11VA de recepção.
