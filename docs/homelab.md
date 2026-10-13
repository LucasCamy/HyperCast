# Compatibilidade com Cloudlane Homelab

Fonte analisada: `LucasCamy/homelab-docs`, commit `01092ae5ea7ce61b8cf3d3bbd43037549b93b155`. A skill `cloudlane-homelab` foi lida e instalada localmente. Foram consultados hardware/topologia, guia de deploy e catálogo de portas. Credenciais do homelab não foram utilizadas.

A VM 200 é documentada com quatro vCPUs e 16 GB, endereço LAN `192.168.15.19`. A faixa `8100+` consta como disponível no catálogo consultado. As portas 8100–8105 são uma proposta local: não foram reservadas no catálogo remoto nem verificadas em serviços ativos. Antes de deploy, conferir conflitos e registrar a reserva no catálogo. Há divergência de RAM entre o resumo da skill (32 GB no host) e o manual de hardware (64 GB); confirmar a máquina atual em vez de dimensionar com base no resumo.

Para esta primeira etapa, manter Compose em uma VM Linux dedicada ou no host de aplicação escolhido. Não instalar Docker diretamente no hipervisor Proxmox como consequência deste esqueleto. Migrar para k3s exige decidir entre `hostNetwork` e exposição UDP direta; um Ingress HTTP não transporta a mídia RTP.

O túnel Cloudflare existente pode ser considerado para a sinalização HTTP/WebSocket, sujeito a autenticação compatível com cliente nativo. O caminho UDP do SFU exige alcance de rede separado: LAN, VPN até o servidor ou IP/portas públicos com NAT/ICE e TURN conforme necessário. O tráfego da aplicação permanece cliente→SFU→cliente; uma VPN não transforma o aplicativo em uma malha de publicadores.

Capacidade: três publicadores a 45 Mbps somam 135 Mbps de entrada. Para N espectadores recebendo as três telas completas, a saída é aproximadamente N×135 Mbps, mais overhead; seis espectadores chegam a ~810 Mbps. O link gigabit documentado e principalmente o upload WAN devem ser medidos. Destaque e camadas reduzidas são requisitos de capacidade, não apenas escolhas visuais. O SFU não precisa de GPU para encaminhar pacotes; as GPUs dos clientes fazem encode/decode.

Nenhum SSH, deploy remoto, mudança DNS, abertura de firewall ou configuração do túnel foi executado. Os contêineres de validação foram iniciados somente no Docker Desktop local.

As skills `design-taste-frontend` e `ui-ux-pro-max` foram instaladas para a futura interface. `design-motion-principles` já estava instalada e foi consultada. Esta entrega não inclui layout nem animação. A versão atual de taste é voltada a landing pages/portfólios; não deve impor uma stack React ao cliente multimídia nativo. A futura UI deve priorizar área de vídeo, navegação por teclado e pouca animação, sem atrasar a troca Grid/Destaque.
