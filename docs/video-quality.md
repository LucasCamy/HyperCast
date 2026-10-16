# Qualidade de vídeo — 0.8.5

## Limite de upload

Em **Formato de vídeo → Limite de upload · vídeo**, antes de compartilhar:

| Perfil | Teto de vídeo |
| --- | ---: |
| Econômico | 12 Mbps |
| Balanceado, padrão | 25 Mbps |
| Máxima qualidade | 40 Mbps |

A opção funciona em NVIDIA/AMD no Windows e nos encoders GPU do Linux. Ela não
altera a resolução nativa nem o alvo de até 120 FPS. No Linux, o modo Compatível
continua em HEVC 30 FPS e tem teto próprio de 12 Mbps, mesmo selecionando 25/40.
A escolha fica na sessão do aplicativo; ao reabrir, o padrão é 25 Mbps.

O orçamento é usado na inicialização do encoder e no `max_bitrate` da publicação
WebRTC. Reduções solicitadas pela rede continuam valendo abaixo desse teto.
Troca de monitor/janela, recriação do encoder e reconexão preservam o limite.
Para alterar o perfil de upload, pare o envio e inicie novamente. O controle fica
bloqueado enquanto a transmissão ou seleção de captura está ativa.

São limites de bitrate de vídeo, não um limitador de tráfego total do sistema:
áudio, RTP/UDP/TURN e retransmissões acrescentam consumo e pode haver picos.
Menos Mbps pode aumentar a compressão visível em movimento e telas complexas;
o aplicativo não promete manter a mesma fidelidade em todos os perfis.

**Perfil AMD · HEVC** é uma opção independente. O Windows verifica o VendorId
DXGI da GPU da origem selecionada, não apenas a presença de uma GPU no PC.
Esse controle só fica disponível para AMD + HEVC, e fica bloqueado para NVIDIA,
AV1 e GPUs não suportadas. Os limites de upload continuam disponíveis para todos.

## Indicadores na interface

A partir de 0.8.6, a barra abaixo do cabeçalho da sala é igual no Windows e Linux:

- **Envio:** resolução codificada, quadros por segundo e Mbps calculados das
  diferenças dos contadores, com janela mínima de um segundo e duração real.
  São quadros/bytes de vídeo aceitos pelo publicador; excluem overhead RTP/TURN e
  não comprovam entrega ao espectador. Não são os alvos configurados do encoder.
- **Recepção:** total de quadros decodificados por segundo em todas as telas.
  Não é FPS de apresentação ou FPS individual quando várias telas estão ativas.
- **Estado da sala:** conectada, reconectando, envio pausado e recepção pausada.
  Esse estado é separado do indicador de acesso ao servidor.

Antes da primeira amostra aparece **medindo…**. O contador é reiniciado ao
encerrar o envio; a reconexão e a pausa têm prioridade sobre valores anteriores.
A barra fica oculta em tela cheia. Não se atribui nota de qualidade: FPS/bitrate
não medem pixelização, fidelidade ou latência fim a fim. Os detalhes técnicos
continuam no Debug.

## Perfis AMD HEVC

Em **Formato de vídeo → Perfil AMD · HEVC**, antes de compartilhar:

- **Rápido**: padrão, preset Speed, VBV de dois quadros e GOP de um segundo.
- **Texto**: experimental, preset Balanced, VBV de dois quadros e GOP de dois
  segundos. Pode consumir mais GPU e aumentar o tempo de codificação. Se o driver
  rejeitar a inicialização, o aplicativo tenta Rápido e registra o perfil aplicado.
- **Compatibilidade**: Speed, VBV de um quadro e GOP de um segundo.

Os perfis se aplicam ao AMF HEVC no Windows. NVIDIA mantém seu preset; AV1 AMD
usa Rápido e depende do suporte físico da GPU. RX 5700 XT não codifica AV1.
Quadros-chave solicitados por entrada de espectador, recuperação, mudança de
resolução e retomada continuam disponíveis em todos os perfis.

## Ajustes

Mudanças de bitrate não forçam mais um IDR. Solicitações idênticas são ignoradas.
AMF aceita solicitações de 64 kbps a 45 Mbps; o publicador limita o envio a 40 Mbps.
O VBV acompanha bitrate/FPS quando o driver permite sua alteração durante a sessão.
Se a taxa for rejeitada, o envio aguarda outra solicitação em vez de destruir o
encoder. O mailbox de taxas chega mesmo sem novos quadros, permitindo retomar
um envio pausado. Sua referência no encoder WebRTC é fraca e não retém a fonte.

Na reconexão, preserva-se o orçamento anterior; se estava pausado, um quadro de
bootstrap usa 64 kbps para vincular o novo encoder WebRTC ao controle de taxa.
O controle recebido em seguida determina pausa ou retomada.

Decoders Windows e Linux mantêm o pacote quando FFmpeg retorna EAGAIN, escoam
os quadros e tentam enviar o mesmo pacote uma vez. Erros persistentes são
reportados, sem loop ilimitado nem descarte silencioso desse pacote.

## Diagnóstico

Em **Debug → Estatísticas do encoder**, e nos logs `encoder_window` a cada cinco
segundos, estão bitrate solicitado/aplicado, VBV e suporte à alteração em runtime,
FPS alvo, tempo p50/p95, bytes, maior quadro e contagem de quadros-chave.
QP HEVC é amostrado a cada 30 quadros se o driver fornecer feedback; ausência é
registrada como indisponível. Tempo de encoder não mede latência fim a fim.

Logs `video_rtp` incluem contadores cumulativos numéricos: bytes, quadros,
NACK/PLI/FIR, perda, jitter, quedas e congelamentos conforme o SDK disponibiliza.
Compare diferenças entre amostras; os contadores não são uma taxa instantânea.
Não são incluídos tokens, conteúdo da tela, nomes nem endereços nesses registros.

## Evidências e limites

Uma cena sintética branca com barras, 2560×1440, em uma GPU AMD integrada
DEV_13C0 reproduziu perda periódica de fidelidade nos keyframes. Forçar IDRs
extras aumentou os bytes em cerca de 91% nesse ensaio. Balanced e VBV maior
melhoraram a média de PSNR, mas não eliminaram a queda nos keyframes.
Isso orientou as mudanças; não prova a causa exata da sessão da RX 5700 XT.

O teste de hardware `tests/integration_amf.ps1` verifica perfis, taxas sub-Mbps,
retomada após rejeição, ausência de IDR por mudança de taxa, HDR e liberação de
sessões. `tests/integration_amf_window.py` usa janela sintética via WGC, AMF,
SFU e apresentação GPU. Testes de mídia Linux verificam recepção cruzada.

Em 15/10/2026, passaram: 19 testes Rust da aplicação; cinco testes nativos,
incluindo 100 ciclos de retomada/liberação de mailbox; HEVC nos três perfis AMD;
NVENC sem IDR extra nas mudanças de taxa; AMF de janela até apresentação GPU
com zero superfícies retidas após fechar; interrupção real de sinalização com
retomada de vídeo/áudio; HEVC e AV1 entre Linux e Windows. O AV1 Linux foi
validado com o AppImage e seus plugins de mídia, usando hardware NVIDIA.

A comparação final na RX 5700 XT precisa repetir a mesma página branca, texto
e rolagem nos três perfis, anotando bitrate, perda e p95. Não há garantia nova de
120 FPS estáveis nem de latência abaixo de 100 ms. Intra-refresh permanece em
avaliação separada; não foi ativado nesta versão.
