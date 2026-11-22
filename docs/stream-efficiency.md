# Eficiência do envio — medições de 21/11/2026

RTX 4080 (driver 621.24), Windows 11, build release, SFU local. Cliente A transmite
uma janela 2560×1440 em movimento; cliente B assiste. Contadores do Windows por
processo, uma amostra por segundo: `GPU Engine` (3D, VideoEncode, VideoDecode,
Copy, Compute), `GPU Process Memory\Dedicated Usage`, CPU e memória privada.

## Onde a GPU é gasta

| Cenário (A) | NVENC | 3D | CPU | Envio |
|---|---:|---:|---:|---:|
| 1440p 120 FPS, prévia ligada | 18,9% | 2,5% | 0,9% | 20,4 Mb/s |
| 1440p 60 FPS | 9,8% | 3,7% | 0,7% | 17,0 Mb/s |
| 1440p 120 FPS, sem prévia | 18,8% | 4,4% | 0,8% | 20,1 Mb/s |

Quem assiste (B), a 120 FPS: decode 9,9%, 3D 5,2%, CPU 3,1%.

- O "GPU %" do Gerenciador de Tarefas é o motor mais ocupado. Ao transmitir é o
  **VideoEncode**, um bloco dedicado: não disputa com o 3D do jogo. O custo em 3D
  do HyperCast fica entre 2% e 6%.
- O NVENC escala com quadros × pixels: 120 FPS custa o dobro de 60 FPS. Desde
  então o padrão é **Nativa, 60 FPS, Econômico (12 Mb/s)**; 120 FPS e 25/40 Mb/s
  continuam disponíveis em **Formato de vídeo**.
- A porcentagem também depende do clock: com a GPU em P3 (clock de vídeo menor,
  outros apps abertos) a mesma configuração mediu 24–25%. Compare sempre no mesmo
  estado de energia.
- A prévia da própria tela não muda o 3D de forma mensurável; ocupa ~50 MB de VRAM.

## Compressão (NVENC HEVC)

Clipe de 8 s 1440p60 (fractal em zoom + padrão de teste, com um corte de cena),
codificado pelo `hevc_nvenc` do ffmpeg com os mesmos parâmetros e pontuado com
VMAF. É um conteúdo propositalmente difícil; os valores absolutos são baixos.

| Ajuste a 12 Mb/s | VMAF | Pior 1% | Tempo de encode |
|---|---:|---:|---:|
| Atual: P1, IDR a cada 1 s, VBV 1 quadro | 63,7 | 12,3 | 1,8 s |
| IDR a cada 5 s | 63,8 | 13,7 | 1,9 s |
| P3 | 64,4 | 13,7 | 2,2 s |
| P4 | 65,1 | 14,1 | 2,4 s |
| AQ espacial | 63,9 | 13,8 | 2,1 s |

Precisão do bitrate (CBR) com VBV de 1 quadro: 4 Mb/s pedidos viraram 4,93
(+23%) e 8 viraram 9,21 (+15%). Com 2 quadros: 4,00 e 7,98. O excesso aparece
justamente quando a rede está no limite e vira fila/perda; a "qualidade" a mais
com 1 quadro vem só dos bits a mais.

### Aplicado

- **IDR a cada 5 s** em vez de 1 s. PLI/FIR do WebRTC (perda ou novo espectador)
  continua forçando um IDR imediato. Medido: 1 IDR por janela de 5 s, antes 5.
- **VBV de 2 quadros**, como o encoder AMD já usa. Bitrate fica no alvo; até
  +1 quadro de espera no pior caso (8 ms a 120 FPS).
- **P1 mantido**: P3/P4 dariam +0,8/+1,4 VMAF por 20–33% mais tempo no NVENC e
  mais latência por quadro. AQ não compensou.
- Nenhum dos dois ajustes mudou o uso do NVENC (controle medido no mesmo estado
  da GPU: 23,7–25,4% antes, 25,1–25,3% depois).

## Rede

Bytes RTP de vídeo ficaram ~8% acima dos bytes codificados (9,0 MB contra
8,35 MB em 5 s), sem NACK. Esse acréscimo é a sobrecarga normal do RTP/WebRTC.
A banda concedida pelo WebRTC começa em ~6 Mb/s e chega ao teto em ~20 s.

## Memória

- **Captura de monitor:** 16 ciclos de iniciar/parar, VRAM estável (161 MB).
- **Captura de janela: ~15 MB de VRAM presos por início/parada** (uma
  superfície do tamanho da janela): 256 → 916 MB em 47 ciclos no app. A memória
  privada fica estável. Quem assiste não vaza.

### Investigação

O teste `video_capture::leak_tests` (ignorado; precisa de `HYPERCAST_LEAK_PID`)
abre e fecha só a captura de janela, sem encoder nem WebRTC, e lê a VRAM do
próprio processo. Reproduz o mesmo valor: 15 MB/ciclo em 1440p, 3,75 MB em 720p.

| Variante | VRAM presa por ciclo |
|---|---:|
| Abrir a captura, sem sessão | 0 |
| Criar pool, sem sessão | 0 |
| Criar sessão sem `StartCapture` | 0 |
| `StartCapture` sem ler quadros | 15 MB |
| Sem a borda amarela / sem cursor | 15 MB |
| Encolher o pool antes de fechar / fechar pool antes da sessão | 15 MB |
| Um único dispositivo D3D para todas as sessões | 13,5 MB |
| Janela do Bloco de Notas (GDI) em vez de swapchain DirectX | 13,75 MB |

A sequência mínima da API (`CreateFreeThreaded` → `CreateCaptureSession` →
`StartCapture` → `Close`) prende uma superfície do tamanho da janela, mesmo com
o dispositivo D3D chegando a 0 referências. Não depende do nosso encoder, da
cópia, do tipo de janela nem do dispositivo: aponta para o Windows Graphics
Capture/driver (Windows 11 26300, driver NVIDIA 621.24). Ainda não foi testado
em outra GPU ou versão do Windows.

Contornos possíveis, nenhum aplicado: manter a sessão viva ao reiniciar a
mesma janela, ou capturar janelas num processo auxiliar que termina ao parar (o
Windows libera tudo na saída do processo). Enquanto isso, o custo aparece só ao
reiniciar a transmissão de janela; fechar o app libera tudo.

Também corrigido durante a investigação: a captura entrava e saía do
apartamento COM/WinRT a cada sessão. Uma captura aberta e fechada sem iniciar
(por exemplo, quando o encoder falha depois de `prepare`) derrubava o processo
na tentativa seguinte. Agora a thread entra uma vez e permanece.

### Também corrigido

- A captura de janela criava uma textura nova (14,7 MB em 1440p) e uma query a
  **cada quadro**; agora reutiliza as mesmas enquanto o tamanho não muda.
- A prévia local criava uma textura a cada 8 quadros (15/s a 120 FPS); agora
  reaproveita até 3, só quando a interface já soltou a anterior.

## Como repetir

Ganchos de teste (só com `HYPERCAST_SMOKE_SECONDS`, até 300 s):
`HYPERCAST_SMOKE_FPS=60|120`, `HYPERCAST_SMOKE_PREVIEW=0` e
`HYPERCAST_SMOKE_SHARE_CYCLES` (até 48 ciclos de iniciar/parar a cada 3 s).
