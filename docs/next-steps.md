# Revisão de pendências — 13/10/2026

## Atualização executada

Logs automáticos, recuperação limitada, convites/entrada automática e TURN/TLS
foram implementados e testados; detalhes em validation.md, invitations.md e
turn-tls.md. Após ajuste solicitado pelo proprietário, todos os membros do
convite padrão podem publicar; até três telas pendentes/ativas por sala, sem
reservar vagas ao entrar. Teste real comprovou liberação ao parar sem desconectar.

AMF não tem GPU AMD disponível para validação, conforme resposta mais recente
do proprietário. Miniaturas vivas e medição fim a fim continuam pendentes; as
miniaturas atuais permanecem estáticas. A avaliação abaixo registra o plano inicial.

## Evidência do amigo com RTX 3090

Analisados os seis diretórios em LogsTUburuHypercast fornecidos pelo proprietário.
Os arquivos brutos permanecem em Downloads, fora do Git. Todos indicam RTX 3090,
driver 32.0.16.1714 e monitor 1920×1080 a 144 Hz.

Quatro tentativas terminaram em `Query low latency preset: NVENC status 8`.
Duas publicaram uma faixa na sala qhd-test, mas terminaram em
`Capture texture/device changed; rebuild the encoder`, sem contadores de mídia.
Esses arquivos não comprovam uma sessão concluída nem medem FPS ou latência.
O proprietário relata funcionamento posterior pela interface; essa versão da
interface ainda não persiste logs automaticamente.

A proteção do encoder compara device, dimensões e formato da textura.
O erro agregado não identifica qual atributo divergiu. Próxima correção:
diagnóstico discriminado e reconstrução controlada do pipeline quando houver
mudança de resolução/device, com descarte das texturas antigas e recuperação
de publicação. Não remover a proteção ou presumir falha do driver.

## Entrada na sala sem copiar JWT

O emissor atual é uma ferramenta administrativa de desenvolvimento. A interface
deve pedir nome e convite de sala, ou usar login, e obter credenciais de conexão
via HTTPS de um serviço autenticado. JWTs curtos continuam sendo usados pelo
LiveKit; a API secret fica exclusivamente no servidor.

Para uso entre amigos, começar com convite aleatório limitado à sala, com
validade, revogação e número de usos. Guardar somente hash do convite no banco,
não registrar convite/token em logs e aplicar limites de requisições. O backend
define identidade e permissões, sem aceitar grants arbitrários do cliente.
Vagas devem ser atribuídas transacionalmente pelo serviço central, preservando
o pool hc-screen-1..3 e a quota síncrona do SFU. Repetições precisam reutilizar a
reserva da mesma sessão, em vez de consumir outra vaga. Uma credencial de sessão
do aplicativo permite solicitar novo JWT para uma nova entrada.

Não aumentar indefinidamente a validade do JWT nem embutir API secret no cliente.
A expiração do token inicial não significa que uma sala conectada encerra em
cinco minutos; LiveKit tem atualização de token durante a sessão. Exercitar
renovação e reconexão no SDK Rust utilizado antes de prometer recuperação.

Referências oficiais:
- https://docs.livekit.io/frontends/build/authentication/endpoint/
- https://docs.livekit.io/frontends/reference/tokens-grants/

## Ordem de implementação e critérios

| Prioridade | Trabalho | Critério de conclusão / dependência |
|---|---|---|
| 1 | Logs automáticos e exportação na interface | Arquivos limitados/rotacionados, token e convites redigidos, versão do build, GPU/codec, conexão, rota ICE e erros; sem gravação de tela |
| 1 | Recuperação DXGI/encoder | Erro discriminado, troca de modo/resolução exercitada e retorno do compartilhamento sem reiniciar o aplicativo |
| 1 | Convites e emissão automática | Backend autenticado, quota concorrente, convites revogados/expirados recusados, interface entra sem Python/PowerShell |
| 2 | TURN/TLS | DNS-only turn.hypercast.cloudlane.com.br, certificado com renovação, porta TCP dedicada acessível, teste relay-only e credenciais geridas pelo LiveKit; precisa configurar DNS/certificado e verificar porta 443 na VM antes de reservá-la |
| 2 | Medição fim a fim | Movimento QHD contínuo, janela efetiva do receptor, frames distintos, jitter/perda e latência captura→apresentação p50/p95/p99; RTT não substitui esta medição; método óptico como confirmação |
| 3 | Miniaturas vivas | Escala na GPU e representação reduzida, orçamento de FPS/bitrate, pausa de telas invisíveis; SFU não transcodifica; representações devem preservar limite de três compartilhamentos lógicos |
| 3 | AMF | Encoder AMD consumindo textura GPU, negociação HEVC/AV1 conforme capacidades, validação em GPU AMD com monitor ativo; iGPU inativa dos logs não comprova suporte |
| 4 | Operação | Reconexão prolongada, atualização de WAN dinâmico, monitoramento, teste de capacidade de upload com múltiplos participantes e assinatura do instalador |

HTTPS/WSS público e vídeo por UDP público já foram exercitados. TURN/TLS é uma
rota alternativa para redes restritivas, não uma correção para endereço de
sinalização incorreto. Cloudflare Tunnel HTTP atende à sinalização; não substitui
transporte RTP/TURN dos clientes desktop pela internet.
