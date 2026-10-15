# Recuperar a recepção · 0.7.3

Use o botão **↻ Reconectar recepção**, ao lado de **Pausar vídeo** e **Sair**, ou
pressione **F5**. Com a lateral recolhida a ação aparece no cabeçalho; em tela
cheia fica junto dos controles de volume e saída. No Linux aparece no cabeçalho.

A ação pausa brevemente o recebimento e reinstala os leitores nativos de vídeo
e áudio. O último quadro permanece disponível enquanto o fluxo retoma. Ela
preserva a sala, a seleção, o volume, a pausa escolhida pelo usuário e a
publicação local; não exige novo token nem interrompe seu compartilhamento.
O SFU retoma o envio das faixas selecionadas. O botão fica indisponível durante
o breve período de reinício, evitando pedidos sobrepostos.

## Correção da alternância

As faixas agora permanecem subscritas dentro do limite de três telas. O
cliente usa as configurações de habilitação do LiveKit para o SFU parar o RTP
fora de foco, mantendo a negociação. Isso evita remover e recriar o receptor a
cada clique e depender de uma nova notificação de track. A troca de foco
não cria sucessivas negociações; áudio e vídeo fora de foco ficam pausados.

Eventos antigos de remoção são ignorados quando a publicação já possui uma
track atual. A leitura usa a track atual da publicação, não uma cópia de evento
que pode ter envelhecido. Uma reconciliação periódica restaura leitores ausentes
ou encerrados, sem enviar pedidos de subscrição repetidos para o SFU.

A fila de vídeo no Windows passou a ter um quadro, assim como a do Linux.
Quadros de tracks desabilitadas não sobrescrevem a miniatura com imagem preta.
No Linux as sequências de quadro são monotônicas ao reinstalar leitores, para
a textura da interface reconhecer o primeiro quadro novo. Os leitores de áudio
inativos são liberados para não manter reprodução/trabalhadores desnecessários.

## Validação

`tests/integration_reception.py` usa uma janela FFplay gerada e dois publicadores
Linux sintéticos numa sala isolada: 41 trocas rápidas, três reinícios de recepção,
quadros novos por publicador, identificador da publicação local preservado e
zero superfícies D3D11 restantes após o encerramento. Nenhum desktop pessoal é
capturado. `tests/integration_linux_reception.py` executa um workload Rust
ignorado por padrão com 60 trocas, três reinícios e liberação dos decodificadores.
Os testes dependem de credenciais locais de operador; não armazenam tokens no
repositório e removem a sala ao terminar.

O botão recupera a leitura e os fluxos pausados. Uma falha de conectividade
continua sujeita à recuperação da conexão WebRTC; ele não contorna indisponibilidade
do servidor ou da rede. Nenhuma mudança no servidor é necessária nesta versão.
