# Acesso ao servidor, salas salvas e permanentes — 0.8.2

Primeiro informe a **Chave de acesso** compartilhada e use **Entrar no servidor**.
Essa chave autoriza usar o HyperCast: criar salas e escolher as salas da sua lista.
Não é a chave administrativa do LiveKit. Cada amigo usa seu próprio apelido.

Depois use **Escolher sala**, ao lado de **Criar sala**. A lista mostra os nomes,
a quantidade de participantes e a permanência. Clique **Entrar** no cartão da
sala; não é necessário digitar nem copiar um código. A janela usa a altura útil
do aplicativo e rola verticalmente para mostrar mais salas.

Para criar, informe nome da sala, seu nome e a opção **Manter sala indefinidamente**.
O nome da sala aceita espaços e acentos (1–64 caracteres); seu apelido aceita
letras sem acentos, números, hífen e sublinhado. **Criar e entrar** abre a sessão
automaticamente. Todos podem compartilhar: até 12 pessoas e três telas por sala.

**Lembrar neste Windows** protege a chave com DPAPI do usuário, vinculada à origem
do servidor, em LocalAppData/HyperCast/server-access-<hash>.bin. A sessão de acesso
fica somente em memória, dura 15 minutos e é renovada automaticamente com a chave.
**Trocar acesso ao servidor** remove a chave local e encerra essa autenticação;
não revoga a chave compartilhada. Identificadores internos de sala não precisam
ser copiados ou guardados pelo usuário. Credenciais não entram nos logs.

## Entrada pela lista

Use **Escolher sala** para listar as salas da chave compartilhada do servidor,
inclusive as criadas em outro dispositivo com a mesma chave. Saia da sessão
atual antes de entrar em outra. A interface não tem campo de código nem ações
de copiar convite. A entrada usa o ID interno da sala e o servidor valida que
a sala pertence à mesma chave de acesso. Sessão válida e vínculo ao dispositivo
continuam obrigatórios.

Clientes antigos ainda podem usar os endpoints e convites anteriores por
compatibilidade. O fluxo atual não depende de receber um código na listagem.

Ao criar, marque **Manter sala indefinidamente** para não expirar o registro
da sala. A mesma opção está nos cartões da lista; desmarcar inicia um novo prazo
de 24 horas. Criar com
o mesmo nome, sem diferenciar maiúsculas e minúsculas, recupera o registro válido
existente em vez de criar uma duplicata. Os dados ficam no SQLite persistente,
não no cache do aplicativo. A chave de acesso continua sujeita à validade e à
revogação pelo operador; a sala não é apagada quando uma sessão de acesso expira.

Permanência não mantém um processo ou uma conexão de vídeo rodando para sempre.
O SFU libera a sala vazia após dez minutos. A próxima entrada recria a sala com
o mesmo nome, identificador e limites. A interface usa o nome retornado pela API de
entrada, evitando exibir o identificador interno `hcr-...` por falta de metadata.

## Limpeza automática

O servidor verifica as salas a cada minuto, e também ao carregar a lista.
Temporárias que expiraram e estão vazias têm seus registros, convites e membros
removidos automaticamente do banco. Aguarda-se a validade residual máxima de
cinco minutos dos tokens de entrada antes da remoção. Salas permanentes e salas
com participantes são preservadas. Se o SFU estiver indisponível, a limpeza é
adiada: não se presume que uma sala esteja vazia sem consultar o servidor.

A lista aberta se atualiza a cada 30 segundos. Salas expiradas que ainda tenham
participantes permanecem visíveis até ficarem vazias e cumprirem o prazo de
segurança. O serviço não desconecta participantes nem apaga salas permanentes
por estarem sem transmissão. O SFU continua liberando suas instâncias vazias
pelo timeout normal. As operações de remoção no SQLite são transacionais e
limitadas a 128 salas por execução.

## Limites e validade

- Chave compartilhada: validade definida pelo operador, inicialmente 30 dias.
- Até quatro salas abertas no servidor via API. HYPERCAST_MAX_ROOMS aceita 1–16.
- Até 32 registros de salas válidas por chave compartilhada. Não há mais o
  bloqueio de uma única sala aberta por dispositivo.
- Até dez novas salas por chave compartilhada em 24 horas. Repetir o mesmo pedido
  não cria outra sala nem consome nova quota.
- Sala temporária: 24 horas e até 12 dispositivos distintos durante sua validade.
- Sala permanente: registro sem expiração; não bloqueia o 13º dispositivo
  usado ao longo do tempo. O limite continua sendo 12 pessoas simultâneas no SFU.

A chave compartilhada não identifica pessoas individualmente. As quotas de
salas abertas, registros e criações diárias continuam aplicadas no servidor.
Revogar a chave bloqueia login, novas criações e novas entradas pela API, inclusive
sessões de acesso já emitidas. Sessões LiveKit já conectadas e seus JWTs curtos não
são encerrados automaticamente. O operador pode encerrá-las separadamente.
Chamadas administrativas diretas ao SFU continuam sob responsabilidade do operador.

## Operador e API

```powershell
python -m tools.room_invite --env-file .tools/homelab.env --access-key --label Amigos --days 30 --output .tools/access/amigos.json
python -m tools.room_invite --env-file .tools/homelab.env --revoke-access ID_DO_ACESSO
```

Entregue somente `access_key` aos amigos. Guarde `access_id` para revogação.
Arquivos privados ficam ignorados pelo Git e fora dos instaladores.

`POST /api/login` recebe a chave bearer e somente `device_id` no corpo; retorna
JWT HS256 com audience hypercast-access, scope server:access e expiração de 900s.
`POST /api/rooms` recebe esse JWT e `room_name`, `request_id` e `persistent` (booleano).
`POST /api/rooms/list` recebe esse JWT e um objeto vazio; lista até 32 registros,
priorizando permanentes e válidos. `POST /api/rooms/join` recebe `room`, `name` e `request_id`, e devolve JWT curto
e nome amigável após verificar que a sala pertence à mesma chave.
`POST /api/rooms/keep` recebe `room` (ID interno)
e `persistent`; só altera salas da mesma chave de acesso.
`POST /api/join` recebe o JWT, convite, apelido e request_id igual ao device_id.
O servidor verifica assinatura, escopo, prazo, revogação e vínculo ao dispositivo.
SQLite serializa quotas e criações, guardando somente hashes das chaves/convites.
RoomService permanece interno à API. Pedidos repetidos têm ID e convite estáveis,
inclusive quando se perde uma resposta de criação no SFU.

HYPERCAST_REQUIRE_ACCESS=1 exige login para toda entrada pela API. Clientes 0.5.3
precisam atualizar; os convites anteriores continuam utilizáveis após autenticar.
A migração adiciona `room_options` e a view `room_catalog`, preservando a tabela
original de salas e os convites existentes. Mesmas portas,
hostname e túnel. O rollout atualiza hypercast-access, que hospeda o gateway de
sinalização; deploy_rooms_on_node.py exige sala livre, salvo autorização explícita
do operador com --drain-approved. Mantém snapshot para rollback. Não reinicia SFU/TURN.

## Validação

```powershell
python -m unittest discover -s tests -p 'test_*.py'
python -m tests.integration_room_catalog
$env:HYPERCAST_CATALOG_UI="1"
python -m tests.integration_room_catalog
```

Integrações usam configuração privada, salas isoladas e revogam acessos de teste.
O teste de interface cria uma credencial DPAPI temporária para uma origem
loopback exclusiva, sem sobrescrever a chave do servidor real. Verifica login,
lista, alteração de permanência, entrada com nome amigável, saída e recuperação.
Não publica nem grava tela durante a criação/entrada.
