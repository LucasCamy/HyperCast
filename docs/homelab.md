# HyperCast no Cloudlane Homelab

Salas e exclusão (15/10/2026, 0.8.4): serviço de acesso em
`ghcr.io/lucascamy/hypercast-auth@sha256:522542095d481933ae3299765858b0cd348e93dd8616015315224977cd79475a`.
Backup SQLite realizado antes da atualização. SFU não reiniciado. Salvar não
abre sessão de mídia; limite permanece quatro instâncias. Novas instâncias usam
empty_timeout=60 e departure_timeout=1. Instâncias já existentes mantêm os valores
anteriores até encerrarem. Exclusão de cadastro valida ocupação e não força DeleteRoom.
Ledger separado conserva a quota de dez criações/dia mesmo após exclusão.

Limpeza do catálogo (15/10/2026, 0.8.2): serviço de acesso atualizado para
`ghcr.io/lucascamy/hypercast-auth@sha256:9dfb3804755f9d3f08f53219af606b4930a6c4632aa14b45371b402e6c36effb`.
Backup SQLite realizado antes da atualização. A primeira execução removeu 15 salas
temporárias expiradas e vazias. Verificação a cada minuto, com tolerância de cinco
minutos após expiração para os tokens de entrada; falha ao consultar o SFU adia a
limpeza. Salas permanentes e ocupadas são preservadas. SFU e TURN não foram alterados.

Entrada pela lista (15/10/2026, 0.8.1): novo `/api/rooms/join` aceita ID interno
e valida acesso à sala pela mesma chave compartilhada. Clientes antigos e suas
rotas permanecem compatíveis. Imagem de acesso instalada:
`ghcr.io/lucascamy/hypercast-auth@sha256:6e593b1fe2e1f57f1103586953dd3d0c8cc3f1e1fd503e3c16f260e22f90a88a`.
Backup SQLite antes do rollout; sem alteração em SFU, TURN ou portas.

Salas salvas (15/10/2026, cliente 0.8.0): API de listagem e permanência,
códigos de oito letras para novas salas e nome amigável retornado no join.
Imagem de acesso instalada: `ghcr.io/lucascamy/hypercast-auth@sha256:c218239ca1c507f4713720ed02ce4a589ff8ceb8ea63f6ae9e907bf74dd059b4`.
Backup SQLite privado `pre-catalog-*.sqlite3` no PVC `/data`; migração aditiva
com `room_options`/`room_catalog`, sem apagar registros antigos. Mantém SFU,
TURN e portas. [Como usar](room-creation.md).

Correção de rede (15/10/2026): SFU e DNS TURN reconciliados após troca do IPv4
da operadora. `hypercast-network-sync.timer` ativo na VM, com verificação a cada
dois minutos. Atualiza o registro A DNS-only do TURN e o candidato público do
SFU; não reinicia se os endereços continuarem iguais.
Veja [IP dinâmico e Cloudflare](dynamic-public-ip.md).

Atualização de salas (14/10/2026): API com criação autorizada, SQLite persistente,
metadata amigável e limite inicial de quatro salas via API. Imagem importada e
fixada em `ghcr.io/lucascamy/hypercast-auth@sha256:5e24d7e24c2a3b064c5fe8a02eb48eb202dc7305554a2119be14c513ea4a2956`.
Apenas hypercast-access foi atualizado, com interrupção autorizada de sessões.
Login compartilhado em /api/login; sessões de 15 minutos e limite por dispositivo.
HYPERCAST_REQUIRE_ACCESS=1 ativo: entradas pela API exigem sessão de acesso.
Backup privado em /etc/hypercast/access-login-rollback.json (0600).
SFU, TURN e portas iguais.
Veja [criação de salas](room-creation.md).

Atualização de áudio (13/10/2026): SFU e API de convites atualizados após o
proprietário liberar a sala. `audio/opus` habilitado e até três subscrições de
áudio. Uma faixa `screen_share_audio` por tela, recusando áudio isolado,
duplicatas e microfone. Mesmas portas, TURN, DNS e convites existentes.
Imagens importadas/fixadas no nó: SFU
`ghcr.io/lucascamy/hypercast-sfu@sha256:23970932c5444605e07d37efc5488c2c0848fea4ffdab13dda05479fb629fd8e`
e API
`ghcr.io/lucascamy/hypercast-auth@sha256:cfad7711c9360a0e42ec9b6298d793a0a6edc88ce3ee29f980bab72e41bc8591`.
Rollback privado no nó em `/etc/hypercast/audio-rollback.json` (0600).
Detalhes e limites em [áudio do sistema](system-audio.md).

A análise inicial usou LucasCamy/homelab-docs e a skill cloudlane-homelab. Em 13/10/2026, após autorização do proprietário e liberação da VPN, a VM 192.168.15.19 foi inspecionada e o SFU instalado no k3s. Nenhum software foi instalado no hipervisor PVE.

## Implantação executada

Atualização: gateway/convites e TURN/TLS agora estão ativos; ver
[convites](invitations.md) e [TURN](turn-tls.md). Gateway ocupa TCP 8100; SFU
interno usa 8107, mantendo ICE/RTP nas portas originais. API loopback 8106.
TURN TCP 5349 e relay UDP interno 8110–8209. Não abrir portas de API no roteador.
Imagens da API/SFU foram importadas no nó por digest; coturn/Caddy usam imagens
oficiais por digest. Quota atual é por publicação, não por entrada de membro.

- Nó Kubernetes vm-200, Ubuntu 24.04, k3s v1.36.5+k3s1; namespace hypercast.
- LiveKit customizado 1.13.9 com quota síncrona de até três publicadores. Imagem local testada importada para containerd e referenciada por digest.
- Deployment de uma réplica, hostNetwork, não-root, filesystem somente leitura, probes e Service ClusterIP. TCP 8100/8101 e UDP 8102–8105 conferidos antes de reservar.
- API key/secret aleatórios exclusivos em Secret hypercast-livekit. Ambiente administrativo e SQLite de emissão ficam fora do Git.
- Tetos de buffers rmem_max/wmem_max de 16 MiB na VM, persistidos em /etc/sysctl.d/90-hypercast.conf.
- Hostname hypercast.cloudlane.com.br cadastrado pelo proprietário no Cloudlane Tunnel existente, origem http://192.168.15.19:8100. HTTPS 200 e WSS autenticado validados.

A descoberta STUN encontrou o WAN, mas sua auto-validação falhou atrás do NAT. Foi configurado IPv4 WAN explícito com candidatos internos habilitados e filtro para o IPv4 da VM. WAN dinâmico exige atualizar ConfigMap e reiniciar após drenar salas.

## Evidência

Preparação TURN em 13/10/2026: DNS-only turn.hypercast.cloudlane.com.br criado
na zona Cloudflare apontando ao WAN atual; certificado Let's Encrypt DNS-01
emitido na VM e timer Certbot ativo. Operador informou encaminhamento TCP 5349
para a VM. Serviço coturn e integração no SFU ainda pendentes; ver
[preparação TURN/TLS](turn-tls.md). Credenciais DNS ficam protegidas e fora do Git.

Quota real rejeitou segundo screen_share e identidades fora do pool. AV1 via SFU remoto passou com 650 quadros D3D11; depois WSS público passou com 591 quadros. São amostras funcionais 1080p, sem comprovar estabilidade QHD/120 ou latência. Sinalização atravessou Cloudflare; mídia usou a rota LAN/VPN disponível.

## Rota pública validada

O proprietário encaminhou TCP 8101 e UDP 8102–8105 no roteador para a VM, preservando portas. TCP público respondeu. AV1 e HEVC passaram por UDP para candidato não privado, com 658 e 653 quadros D3D11 e RTT de rede de 1–3 ms. A rota local até o WAN usou Ethernet, sem depender da rota privada da VPN. Nenhuma sala estava ativa ao iniciar o teste; candidatos internos foram temporariamente retirados para forçar o caminho público e restaurados ao final. Não encaminhar 8100. HTTP Tunnel não substitui ICE/RTP. TURN/TLS para redes restritivas ainda não foi instalado.

Três publicadores a 45 Mbps somam 135 Mbps de entrada; cada espectador em Grid pode exigir mais 135 Mbps de saída, além de overhead. Medir upload WAN antes de prometer capacidade. SFU encaminha pacotes sem GPU; GPUs dos clientes fazem encode/decode.

Manifests, operação, imagens, tokens e recuperação: [k8s/README.md](../k8s/README.md). O catálogo de portas e a matriz DNS de homelab-docs receberam a reserva da aplicação. Monitoramento remoto em Uptime Kuma/Beszel ainda pendente.
