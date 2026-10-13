# Preparação TURN/TLS no Cloudlane

## Restrição confirmada pelo proprietário

O roteador Vivo reserva WAN TCP 443 para gerenciamento da operadora e não
permite encaminhá-la. A regra 443 descrita abaixo é o plano original, atualmente
inviável nesse roteador; não tentar desabilitar o gerenciamento da operadora.

O Tunnel HTTPS existente continua atendendo à sinalização. Publicar TURN como
HTTP nesse túnel não cria um endpoint TURN público para o cliente atual.
O roteamento TCP por hostname com cloudflared requer um conector no cliente;
WARP também altera os requisitos de instalação/rota dos amigos.

Alternativas para implementação:

1. TURN externo (coturn) na VM, com TLS em WAN TCP 5349, DNS-only e certificado
   público. Anunciar explicitamente host/porta em rtc.turn_servers do LiveKit e
   usar credenciais temporárias geradas pelo SFU. Redes que bloqueiam 5349 podem
   continuar falhando. Dimensionar portas de relay e firewall separadamente.
2. TURN externo em VPS, TLS WAN TCP 443, preservando o SFU no homelab. Exige
   acesso à VPS e considerar latência e tráfego de saída do relay.

Na versão LiveKit 1.13.9, iceServersForParticipant anuncia o TURN embutido com
porta pública 443 mesmo quando turn.tls_port usa outra porta de escuta. Portanto
alterar apenas turn.tls_port para 5349 não resolve. O caminho rtc.turn_servers
aceita host e porta explícitos para TURN externo.

Referências da revisão:
- https://developers.cloudflare.com/tunnel/concepts/routing/
- https://raw.githubusercontent.com/livekit/livekit/v1.13.9/pkg/service/roommanager.go

Nenhuma dessas alternativas foi instalada. Um token Cloudflare enviado no chat
deve ser revogado e substituído por outro guardado em arquivo local protegido.

TURN/TLS ainda não está implantado. Sinalização já usa
wss://hypercast.cloudlane.com.br pelo Cloudlane Tunnel. Manter esse hostname.

## DNS e roteador

No painel Cloudflare, selecionar a zona cloudlane.com.br → DNS → Records → Add record:

| Campo | Valor |
|---|---|
| Type | A |
| Name | turn.hypercast |
| IPv4 | IPv4 público atual da conexão Vivo |
| Proxy status | DNS only, nuvem cinza |
| TTL | Auto |

O nome completo deve resultar em turn.hypercast.cloudlane.com.br. Não criar
Public Hostname no túnel para esse endereço: os clientes TURN precisam chegar
diretamente ao endpoint de mídia. Não apontar ao IP privado 192.168.15.19.
Em 13/10/2026 o IPv4 observado da VM era 179.181.177.175; confirmar o WAN ao
configurar, porque ele pode mudar. Não criar AAAA sem rota IPv6 funcional testada.

Encaminhar TCP WAN 443 → 192.168.15.19 TCP 443. Em inspeção de 13/10/2026 não
houve listener TCP 443 nem Service LoadBalancer/NodePort nessa porta no nó;
rever listeners, hostPorts e catálogo antes do deploy. Não configurar regra se
443 já encaminha a outro serviço no roteador. Manter TCP 8101 e UDP 8102–8105.
Não expor o TCP 8100 diretamente.

## Certificado

O TURN precisa de certificado de autoridade pública válido para
turn.hypercast.cloudlane.com.br. Certificado Cloudflare Origin CA não é apropriado
para clientes diretos com trust store padrão.

Proposta: Let's Encrypt DNS-01 com renovação automática. O cluster ainda não
possui pods cert-manager. Para configurar o solver Cloudflare, criar API Token:

- Zone → DNS → Edit.
- Zone → Zone → Read.
- Zone Resources: Include → Specific zone → cloudlane.com.br.

Salvar o token em arquivo local protegido, fora do repositório; informar ao
operador apenas o caminho, sem colar o valor no chat. Não usar Global API Key.
Essa permissão permite alterar DNS da zona e emitir certificados para ela.
DNS-01 evita abrir porta 80 para o desafio. Certificado e chave privada serão
montados por Secret no SFU; é necessário planejar recarga/reinício após renovação.
Não basta criar o registro A para ativar TURN.

## Implantação posterior

Ativar TURN embutido no LiveKit, montar certificado/chave e anunciar o domínio
e TLS 443. Resolver bind privilegiado preservando serviço não-root e capacidades
mínimas, sem conceder privileged. Validar renovação e regra de firewall. Testar
com cliente em outra rede, sem VPN, forçando relay-only, e comprovar candidato
relay/TCP TLS, vídeo e reconexão. UDP direto continua preferível quando disponível.
TURN não garante latência abaixo de 100 ms.

Referências:
- https://docs.livekit.io/transport/self-hosting/ports-firewall/
- https://docs.livekit.io/transport/self-hosting/kubernetes/
- https://developers.cloudflare.com/dns/proxy-status/
- https://cert-manager.io/docs/configuration/acme/dns01/cloudflare/
