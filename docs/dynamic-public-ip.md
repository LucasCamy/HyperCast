# IPv4 público dinâmico e Cloudflare

O túnel Cloudflare mantém a sinalização WSS acessível mesmo quando a operadora
troca o IPv4. A mídia WebRTC tem dois destinos que também precisam acompanhar
essa troca: `rtc.node_ip` anunciado pelo LiveKit e o registro A DNS-only de
`turn.hypercast.cloudlane.com.br`. Alterar apenas o DNS não corrige os candidatos
ICE que já contêm o IP antigo.

Em 15/10/2026, os serviços estavam saudáveis, mas anunciavam um IPv4 anterior.
Clientes externos recebiam `wait_pc_connection timed out`, enquanto a rede
interna seguia alcançando o candidato privado. O DNS e a configuração ativa do
SFU foram reconciliados com o WAN atual; as portas TCP 8101 e TLS 5349 já estavam
encaminhadas corretamente.

## Serviço instalado na VM do k3s

- Script: `/etc/hypercast/sync_public_network.py`, root:root, modo 0700.
- Timer: `hypercast-network-sync.timer`, após 60 segundos do boot e a cada dois
  minutos após concluir a execução anterior.
- Credencial: token DNS já existente no arquivo protegido do Certbot
  `/etc/letsencrypt/cloudflare-hypercast.ini`. Nunca é exibido nem enviado ao cliente.
- O token precisa de Zone Read e DNS Edit na zona `cloudlane.com.br`.
- Registro existente: somente A de `turn.hypercast.cloudlane.com.br`, sem proxy,
  TTL 60. O hostname/túnel da sinalização não é alterado.
- Configuração: somente `node_ip` no Secret `hypercast-sfu-turn-config`;
  credenciais TURN, codecs, portas e candidato LAN permanecem preservados.

Dois serviços HTTPS independentes precisam confirmar o mesmo IPv4 global antes
de qualquer alteração. Falhas de consulta não publicam IP vazio, privado ou
discordante. A API Cloudflare usa TLS validado, respostas limitadas e redirects
desabilitados. A atualização do Secret usa `resourceVersion` para recusar edições
concorrentes. Snapshots para rollback ficam privados em `/etc/hypercast` (0600).

Se o IP mudar, o script atualiza o DNS e o Secret e altera uma annotation no
Deployment para reiniciar o SFU. A annotation permite retomar uma execução
interrompida entre a gravação do Secret e o rollout. Se nada mudar, o SFU não é
reiniciado. A tolerância de encerramento do pod é de 30 segundos; uma troca de
WAN já interrompe as rotas externas existentes, e as salas podem reconectar.

O timer não elimina o intervalo entre a troca do IP, sua detecção, o rollout e
a expiração dos caches DNS. Também não resolve CGNAT nem substitui os
encaminhamentos TCP 8101, UDP 8102–8105 e TCP 5349 para `192.168.15.19`.

## Operação

```sh
sudo systemctl list-timers hypercast-network-sync.timer
sudo journalctl -u hypercast-network-sync.service --since '30 minutes ago'
# Conferência sem alterar:
sudo python3 /etc/hypercast/sync_public_network.py
# Conferência e correção imediata:
sudo systemctl start hypercast-network-sync.service
```

Os templates versionados estão em `infra/hypercast-network-sync.service` e
`infra/hypercast-network-sync.timer`. Instale-os em `/etc/systemd/system`, copie
o script de `tools/` para `/etc/hypercast` com dono root e execute
`systemctl daemon-reload` e `systemctl enable --now hypercast-network-sync.timer`.
O serviço precisa ler o kubeconfig local da VM; não deve ser instalado no PVE.

Testes: `python -m unittest tests.test_public_network` verifica preservação da
configuração, recusa de endereços inválidos/discordantes, repetição após gravação
parcial e execução sem mudanças. `tests/integration_turn_media.py` publica um
vídeo sintético e exige recepção GPU usando TURN/TLS nos dois clientes, sem
capturar a tela pessoal. O teste usa uma sala efêmera, removida ao finalizar.

Referência: [atualização de registros pela API Cloudflare](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/edit/).
