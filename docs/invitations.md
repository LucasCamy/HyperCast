# Entrada por convite

A interface pede servidor, nome e convite. POST HTTPS /api/join troca o convite
por JWT de sala válido por cinco minutos. API secret não é enviada ao cliente.
O convite é armazenado somente como SHA-256 no SQLite persistente do backend.
O cliente guarda um ID aleatório de dispositivo, não o convite, em LocalAppData.
Reentrada com o mesmo convite/dispositivo reutiliza o membro e não consome outro uso.

O convite define sala, validade, máximo de dispositivos e permissão de publicar.
Essas permissões não são aceitas do cliente no endpoint público. Todos os membros
do convite padrão podem compartilhar; a quota de telas é aplicada no SFU apenas
ao publicar, e não ao entrar. Até doze participantes e três telas simultâneas.

Desde 0.3.0, convites de publicador concedem `screen_share` e
`screen_share_audio`; microfone continua proibido. Uma faixa de áudio deve
acompanhar uma tela pendente/ativa do mesmo membro e não consome uma quarta
vaga. O SFU rejeita áudio isolado/duplicado e encerra áudio ao fechar sua tela.
Reentre com o mesmo convite para obter as novas permissões. Tokens manuais
anteriores que permitem apenas vídeo precisam ser gerados novamente.

## Operador

```powershell
python -m tools.room_invite --env-file .tools/homelab.env --room amigos --role publisher --days 7 --max-uses 12 --output .tools/invites/novo-convite.json
python -m tools.room_invite --env-file .tools/homelab.env --revoke ID_DO_CONVITE
```

Distribuir apenas o valor invite, por canal privado. Nunca enviar homelab.env.
Endpoint administrativo exige JWT HS256 com audiência hypercast-invites e scope
invites:write, até 120 s, assinado com a chave do servidor. JWT de participante
não pode emitir convites. O emissor administrativo não está no instalador.

Revogação impede novas emissões, incluindo reentrada pelo convite. JWTs já
emitidos permanecem válidos até expirar; participantes conectados não são expulsos
por essa operação. Para remoção imediata, usar o RoomService administrativo do SFU.
Convites e API devem ser usados por pessoas conhecidas; guardam acesso bearer.

## Operação

Pod hypercast-access tem dois containers: Caddy em TCP 8100 e API em loopback
TCP 8106. O mesmo Cloudlane Tunnel encaminha a sinalização ao gateway; /api/*
vai à API e os demais caminhos/WebSocket ao SFU interno TCP 8107. ICE 8101 e
UDP 8102–8105 continuam diretos. Não encaminhar 8100, 8106 ou 8107 no roteador.

Banco /data/admission.sqlite3 no PVC hypercast-admission, local-path, 128 MiB.
Uma réplica Recreate; chave via Secret. Gateway/API sem root, capabilities ou
token Kubernetes. Limites: 4 KiB por request, 32 handlers e 120 POST/min global.
Não há logs de corpos, Authorization, convites ou JWTs. Respostas não são cacheadas.

A imagem da API foi importada no containerd e fixada por digest. Recriar a VM
exige reimportar a imagem ou publicar/configurar pull no registry. render_auth.py
gera workload revisável; activate_auth_on_node.py muda a porta do SFU somente
sem participantes ativos. Restauração deve coordenar gateway e porta do SFU.

## Quota de telas

SFU identifica membros hc-member-<UUID> e mantém reserva atômica por sala e SID
de participante antes do ACK AddTrack. Publicações pendentes e ativas contam;
segunda tela do mesmo participante é recusada. Unpublish/Close liberam a reserva.
Identidades hc-screen-1..3 continuam aceitas para os diagnósticos administrativos
antigos. A API de convites não reserva vagas de tela ao entrar.

Quota é local ao SFU de uma réplica. Migração de publicadores entre salas/nós não
faz parte desta implantação; não habilitar uma topologia distribuída sem adaptar
a admissão. HYPERCAST_MAX_PUBLISHERS aceita 1–3 no SFU.
