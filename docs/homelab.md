# HyperCast no Cloudlane Homelab

A análise inicial usou LucasCamy/homelab-docs e a skill cloudlane-homelab. Em 13/10/2026, após autorização do proprietário e liberação da VPN, a VM 192.168.15.19 foi inspecionada e o SFU instalado no k3s. Nenhum software foi instalado no hipervisor PVE.

## Implantação executada

- Nó Kubernetes vm-200, Ubuntu 24.04, k3s v1.36.5+k3s1; namespace hypercast.
- LiveKit customizado 1.13.9 com quota síncrona de até três publicadores. Imagem local testada importada para containerd e referenciada por digest.
- Deployment de uma réplica, hostNetwork, não-root, filesystem somente leitura, probes e Service ClusterIP. TCP 8100/8101 e UDP 8102–8105 conferidos antes de reservar.
- API key/secret aleatórios exclusivos em Secret hypercast-livekit. Ambiente administrativo e SQLite de emissão ficam fora do Git.
- Tetos de buffers rmem_max/wmem_max de 16 MiB na VM, persistidos em /etc/sysctl.d/90-hypercast.conf.
- Hostname hypercast.cloudlane.com.br cadastrado pelo proprietário no Cloudlane Tunnel existente, origem http://192.168.15.19:8100. HTTPS 200 e WSS autenticado validados.

A descoberta STUN encontrou o WAN, mas sua auto-validação falhou atrás do NAT. Foi configurado IPv4 WAN explícito com candidatos internos habilitados e filtro para o IPv4 da VM. WAN dinâmico exige atualizar ConfigMap e reiniciar após drenar salas.

## Evidência

Quota real rejeitou segundo screen_share e identidades fora do pool. AV1 via SFU remoto passou com 650 quadros D3D11; depois WSS público passou com 591 quadros. São amostras funcionais 1080p, sem comprovar estabilidade QHD/120 ou latência. Sinalização atravessou Cloudflare; mídia usou a rota LAN/VPN disponível.

## Rota pública validada

O proprietário encaminhou TCP 8101 e UDP 8102–8105 no roteador para a VM, preservando portas. TCP público respondeu. AV1 e HEVC passaram por UDP para candidato não privado, com 658 e 653 quadros D3D11 e RTT de rede de 1–3 ms. A rota local até o WAN usou Ethernet, sem depender da rota privada da VPN. Nenhuma sala estava ativa ao iniciar o teste; candidatos internos foram temporariamente retirados para forçar o caminho público e restaurados ao final. Não encaminhar 8100. HTTP Tunnel não substitui ICE/RTP. TURN/TLS para redes restritivas ainda não foi instalado.

Três publicadores a 45 Mbps somam 135 Mbps de entrada; cada espectador em Grid pode exigir mais 135 Mbps de saída, além de overhead. Medir upload WAN antes de prometer capacidade. SFU encaminha pacotes sem GPU; GPUs dos clientes fazem encode/decode.

Manifests, operação, imagens, tokens e recuperação: [k8s/README.md](../k8s/README.md). O catálogo de portas e a matriz DNS de homelab-docs receberam a reserva da aplicação. Monitoramento remoto em Uptime Kuma/Beszel ainda pendente.
