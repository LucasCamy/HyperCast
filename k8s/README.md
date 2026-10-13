# SFU no Cloudlane k3s

Produção recebeu gateway/convites e TURN: HTTP 8100 vai ao gateway; SFU interno
8107, API loopback 8106 e TURN/TLS 5349. Os manifests de bootstrap abaixo não
representam sozinhos o estado atual. Não reaplicar o SFU de bootstrap com listener
8100 enquanto o gateway estiver ativo. Usar render_turn.py/render_auth.py e os
scripts de ativação documentados em [convites](../docs/invitations.md) e
[TURN](../docs/turn-tls.md). Segredos de configuração/TLS permanecem no cluster.

Implantação de nó único na VM Kubernetes, sem instalar software no hipervisor PVE. Renderizar os manifests em JSON permite validá-los com a API real do cluster; JSON também é YAML válido.

```powershell
python tools/render_k8s.py --image ghcr.io/lucascamy/hypercast-sfu@sha256:DIGEST --node vm-200 --node-ip 192.168.15.19 --public-ip IPV4_PUBLICO --output .tools/hypercast-k8s.json
```

Substituir DIGEST pelo digest real publicado/importado e IPV4_PUBLICO pelo endereço WAN do roteador. O gerador rejeita tags mutáveis, IP público privado e quotas fora de 1–3. `--internet` usa descoberta STUN quando `--public-ip` não foi fornecido; descoberta não configura NAT. Com IP WAN dinâmico, atualizar configuração e reiniciar o SFU após mudança, drenando salas primeiro. `advertise_internal_ip` permite clientes LAN/VPN; apenas o IPv4 escolhido do nó é coletado para candidatos, excluindo interfaces CNI e VPN auxiliares.

Antes de aplicar, conferir sockets, Services e reservas de portas no nó. O Deployment usa `hostNetwork`, uma réplica e estratégia Recreate: não criar um Klipper LoadBalancer para as mesmas portas. O Service de sinalização é ClusterIP. Restrição de nó evita mover o pod para outra VM sem sua imagem/rota. UID não-root, capabilities removidas, filesystem somente leitura e sem token de ServiceAccount.

Criar Secret `hypercast-livekit` no namespace `hypercast`, chave `keys`, com valor `API_KEY: API_SECRET` aleatório exclusivo desse servidor. Nunca versionar o Secret ou credenciais. O operador mantém ambiente privado e banco SQLite persistente para emissão/reservas. O SFU não precisa de banco/PVC nesse modo; reiniciar o pod encerra salas.

```bash
kubectl apply --dry-run=server -f hypercast-k8s.json
kubectl apply -f hypercast-k8s.json
kubectl rollout status deployment/hypercast-sfu -n hypercast --timeout=60s
kubectl get pods -n hypercast
```

ConfigMap atualizado precisa de restart explícito do pod. Drenar sessões antes de atualizar. Probes HTTP validam processo/sinalização; não comprovam ICE ou vídeo pela internet. Os buffers opcionais em `infra/90-hypercast.conf` são aplicados na VM Linux; os valores são tetos de socket.

## Imagem e atualização

Workflow manual `Publish SFU image` compila linux/amd64, executa testes Go da quota e publica no GHCR com tag de revisão. O digest aparece no resumo do job. Usar `--image-pull-secret keel` para imagens privadas, após criar no namespace um Secret do tipo kubernetes.io/dockerconfigjson com permissão somente de leitura do pacote. Não copiar Secrets Opaque de Keel presumindo que sejam credenciais de registry.

O bootstrap de 13/10/2026 importou a imagem já testada do Docker local para containerd da VM e criou sua referência por digest. Ela está no nó, sem necessidade de credenciais GHCR; reinstalar a VM exige reimportar a imagem ou configurar pull. O Deployment não é gerenciado pelo Keel: atualizações do SFU são explícitas porque reiniciar a única réplica interrompe as salas. Se for habilitado gerenciamento pelo Keel, seguir as anotações da skill e programar a interrupção.

## Cloudflare e mídia

Reutilizar `Cloudlane Tunnel`, hostname `hypercast.cloudlane.com.br`, serviço `http://192.168.15.19:8100`. O cliente conecta em `wss://hypercast.cloudlane.com.br`. O hostname foi cadastrado pelo proprietário. Não cadastrar outro túnel. Cloudflare Access com login de navegador exigiria integração específica para o cliente nativo; a conexão usa JWT de sala validado no LiveKit.

| Rota | Configuração |
|---|---|
| HTTPS/WebSocket | Cloudflare Tunnel → TCP 8100 no nó |
| ICE/TCP | WAN TCP 8101 → 192.168.15.19 TCP 8101 |
| RTP/RTCP mux | WAN UDP 8102–8105 → 192.168.15.19 UDP 8102–8105, mantendo portas |

Não encaminhar 8100 pelo roteador. O túnel publicado HTTP não substitui as portas ICE. TURN/TLS para redes restritivas ainda exige um endpoint direto com certificado e capacidade de tráfego; não foi instalado neste bootstrap. Cloudflare WARP requer software/rotas privadas nos clientes e Spectrum exige verificar elegibilidade/comercial; nenhum deles foi contratado/configurado aqui.

## Tokens administrativos

```powershell
python tools/room_token.py --env-file .tools/homelab.env --db .tools/homelab-admission.sqlite3 --room amigos --name Lucas --role publisher
```

Ambiente contém LIVEKIT_API_KEY, LIVEKIT_API_SECRET e LIVEKIT_URL=https://hypercast.cloudlane.com.br; manter fora do Git e acessível somente ao operador. Usar sempre o mesmo banco para esse SFU. Distribuir apenas os JWTs temporários, por canal seguro. `--env-file` explícito substitui credenciais de desenvolvimento herdadas; arquivo inexistente falha sem fallback.

## Estado e limites

Pod saudável e autenticação/quota testados na VM. Após o proprietário encaminhar as portas, AV1 e HEVC passaram pela rota pública UDP, respectivamente 658 e 653 quadros D3D11. Para essa validação, com nenhuma sala ativa, os candidatos internos foram temporariamente retirados; ao final, a configuração pública+interna foi restaurada. O cliente escolheu UDP para candidato não privado, com rota local Ethernet para o WAN e RTT de rede de 1–3 ms. Isso comprova funcionamento da mídia na rota testada; QHD/120 e latência fim a fim ainda dependem de medição em hardware/rede apropriados. AMF e miniaturas vivas permanecem pendentes no cliente. TURN/TLS é necessário como etapa adicional para redes que bloqueiam os caminhos diretos testados.
