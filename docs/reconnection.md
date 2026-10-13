# Recuperação de conexão — 0.5.2

Durante uma queda temporária, o publicador mantém a captura, o encoder e a
publicação local. O SDK tenta recuperar a sessão até **três vezes**, somando
retomada leve e reinício completo; o contador aparece na lateral. A primeira
conexão à sala não é repetida automaticamente. Cada conexão de sinalização
tem timeout de 10 segundos, com intervalo mínimo de dois segundos entre tentativas; ICE também precisa
recuperar. Três tentativas não significam três segundos.

Ao recuperar, o envio solicita um quadro-chave e retoma vídeo e áudio.
Se as tentativas acabarem, a sessão é encerrada e o usuário pode entrar de
novo. Saída manual, expulsão e rejeição de autenticação continuam encerrando
a conexão, sem contornar as permissões do servidor.

O espectador mantém o último quadro GPU durante a interrupção. Se o servidor
remover/recriar o participante, a tela fica marcada **Reconectando • último
quadro**, com a mesma identidade, até chegar o próximo frame. Sem retomada,
ela é removida após até 60 segundos: o espectador não recebe do SFU o contador
de tentativas de outro cliente. Uma saída completa do participante também
pode conservar o quadro durante essa tolerância. **Parar compartilhamento**
com o participante conectado remove a tela imediatamente.

A retenção usa uma única superfície por tela e respeita o limite de três;
uma nova tela ativa pode substituir a superfície congelada mais antiga.
Não há gravação nem acumulação de quadros durante a queda.

Teste isolado (não reinicia o SFU nem interrompe outras salas):

```powershell
python -m tests.integration_signal_reconnect
$env:HYPERCAST_TEST_RECONNECT_EXHAUST = '1'
python -m tests.integration_signal_reconnect
Remove-Item Env:HYPERCAST_TEST_RECONNECT_EXHAUST
```

O teste exige a configuração privada de operador em `.tools/homelab.env`,
a build desktop e o FFmpeg de desenvolvimento. Não é necessário ao usuário.
