# Disponibilidade do servidor

A partir da 0.7.4, o indicador junto à marca HyperCast mostra a disponibilidade
do servidor configurado, tanto no Windows quanto no Linux. Ele funciona antes
de autenticar, fora de uma sala e durante uma sessão.

- **VERIFICANDO:** primeira consulta ou endereço alterado.
- **ONLINE:** o SFU respondeu à consulta de saúde.
- **OFFLINE:** não foi possível confirmar a disponibilidade do SFU.
- **INVÁLIDO:** o endereço não atende às regras de conexão do aplicativo.

O tooltip explica o resultado, inclusive bloqueio HTTP 403 por proxy/firewall.
Sair de uma sala não coloca o servidor offline. A conexão/reconexão da sala e
os erros de autenticação continuam com seus próprios estados e mensagens.

A consulta usa HTTPS no mesmo endereço do WebSocket, no caminho `/`, que já é
encaminhado ao health check do LiveKit pela configuração atual do Caddy.
Exige HTTP 200 com o corpo `OK`; páginas de login, redirects e erros do
Cloudflare não contam como confirmação de saúde. Não envia chave, JWT,
identificador de dispositivo nem cookies. WS/HTTP é permitido apenas em loopback.

O teste inicia depois de 600 ms sem alteração no endereço. Há no máximo uma
consulta em andamento, timeout de quatro segundos e intervalo de 15 segundos
após o resultado. A interface permanece responsiva; respostas de um endereço
anterior são descartadas. A última indicação permanece visível durante as
reverificações. Uma queda ou recuperação pode levar até cerca de 19 segundos
para aparecer.

**ONLINE confirma a disponibilidade da sinalização SFU**, sem garantir login,
participação em sala, conectividade RTP/TURN ou qualidade/latência do vídeo.
Não é necessário alterar nem reiniciar o servidor para usar essa versão.

Validação: testes Rust de resposta válida, HTML inesperado, HTTP 503, redirect,
HTTP 403, ausência de credenciais e descarte de resposta antiga. O teste
`tests/integration_server_status.py` usa uma janela própria do HyperCast e um
servidor HTTP local controlado para verificar **ONLINE → OFFLINE → ONLINE**
sem autenticar nem entrar em uma sala.
