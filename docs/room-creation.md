# Acesso ao servidor e criação de salas — 0.6.0

Primeiro informe a **Chave de acesso** compartilhada e use **Entrar no servidor**.
Essa chave autoriza usar o HyperCast: criar salas e entrar em salas por convite.
Não é a chave administrativa do LiveKit. Cada amigo usa seu próprio apelido.

Depois use **Criar sala**, ao lado de **Entrar na sala**. A janela pede somente
nome da sala e seu nome. O nome da sala aceita espaços e acentos (1–64 caracteres);
seu apelido aceita letras sem acentos, números, hífen e sublinhado.
**Criar e entrar** abre a sessão automaticamente. **Copiar convite** permite
convidar amigos, que precisam autenticar no servidor antes de usar esse convite.
Todos podem compartilhar: até 12 pessoas e três telas simultâneas por sala.

**Lembrar neste Windows** protege a chave com DPAPI do usuário, vinculada à origem
do servidor, em LocalAppData/HyperCast/server-access-<hash>.bin. A sessão de acesso
fica somente em memória, dura 15 minutos e é renovada automaticamente com a chave.
**Trocar acesso ao servidor** remove a chave local e encerra essa autenticação;
não revoga a chave compartilhada. Convites ficam em memória e na área de
transferência quando copiados pelo usuário. Credenciais não entram nos logs.

## Limites e validade

- Chave compartilhada: validade definida pelo operador, inicialmente 30 dias.
- Uma sala aberta por dispositivo autenticado e até quatro salas abertas no
  servidor via API, incluindo salas anteriores. HYPERCAST_MAX_ROOMS aceita 1–16.
- Até dez novas salas por chave compartilhada em 24 horas. Repetir o mesmo pedido
  não cria outra sala nem consome nova quota.
- Convite: 24 horas, até 12 dispositivos distintos. Reentrar pelo mesmo dispositivo
  não consome outro uso. Salas vazias aguardam dez minutos antes de fechar.

O limite por dispositivo organiza o uso entre amigos; não é uma identidade de
pessoa verificada. A quota global e diária continuam aplicadas no servidor.
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
`POST /api/rooms` recebe esse JWT e somente `room_name` e `request_id`.
`POST /api/join` recebe o JWT, convite, apelido e request_id igual ao device_id.
O servidor verifica assinatura, escopo, prazo, revogação e vínculo ao dispositivo.
SQLite serializa quotas e criações, guardando somente hashes das chaves/convites.
RoomService permanece interno à API. Pedidos repetidos têm ID e convite estáveis,
inclusive quando se perde uma resposta de criação no SFU.

HYPERCAST_REQUIRE_ACCESS=1 exige login para toda entrada pela API. Clientes 0.5.3
precisam atualizar; os convites anteriores continuam utilizáveis após autenticar.
A migração adiciona tabelas sem apagar convites existentes. Mesmas portas,
hostname e túnel. O rollout atualiza hypercast-access, que hospeda o gateway de
sinalização; deploy_rooms_on_node.py exige sala livre, salvo autorização explícita
do operador com --drain-approved. Mantém snapshot para rollback. Não reinicia SFU/TURN.

## Validação

```powershell
python -m unittest discover -s tests -p 'test_*.py'
python -m tests.integration_room_creation
python -m tests.integration_room_creation_gui
```

Integrações usam configuração privada, salas isoladas e revogam acessos de teste.
O teste da interface recusa sobrescrever uma chave de usuário já salva e verifica
login, criação sem chave na janela, convite, outro membro e proteção DPAPI.
Não publica nem grava tela durante a criação/entrada.
