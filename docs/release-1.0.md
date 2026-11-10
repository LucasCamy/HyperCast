# Critérios para lançar o HyperCast 1.0

A versão 1.0 deve passar pelos fluxos abaixo com o mesmo commit do cliente e
a mesma imagem do servidor que serão distribuídos. Use uma sala isolada e
conteúdo de teste. Registre a plataforma, o hardware, os pacotes usados e
o resultado de cada linha. Uma aprovação em uma plataforma não aprova a outra.

| Área | Critério de aceite | Evidência disponível / próxima verificação |
| --- | --- | --- |
| Entrar e sair | Criar uma sala, entrar pela lista e por convite, sair e entrar novamente. Uma chave inválida tem mensagem no formulário. Cancelar a saída durante um envio preserva a sessão. | Entrada local e saída verificadas no Windows. Repetir com ambos os pacotes candidatos, incluindo convite e erro de chave. |
| Escolher e parar | Compartilhar um monitor e uma janela com conteúdo em movimento. Trocar de fonte, cancelar uma troca e parar o compartilhamento. O espectador vê a fonte certa e o áudio associado acompanha a tela. | Janela e cancelamento verificados no Windows. Verificar monitor, áudio e seletor do sistema em Linux físico com Wayland. |
| Tela e câmera | Enviar tela HEVC e câmera H.264 do mesmo participante. Confirmar os dois cartões, movimento em ambos e retorno da câmera após desligar e religar, com a tela atualizando. | Codecs e retorno do cartão validados com a Logitech C922. Falta verificar movimento e fluidez presencialmente no pacote candidato. |
| Limites e qualidade | Receber três telas reais e três câmeras. A quarta tela é recusada. Ao parar uma tela, uma câmera pode ocupar a vaga de vídeo. Verificar leitura de texto e fluidez com movimento. | O cenário de seis vídeos e reaproveitamento de vaga passou com fontes geradas. Falta o cenário com equipamento real. |
| Interoperabilidade | Windows assiste a um Linux e Linux assiste a um Windows com tela e câmera. O áudio do sistema e os controles de recepção funcionam nos dois sentidos. | Repetir com os binários candidatos e o SFU atualizado. |
| Recuperação | Pausar e retomar vídeo no espectador. Interromper a rede e restaurar. O app indica reconexão, preserva ou identifica o último quadro e volta a mostrar movimento. | Testes automatizados cobrem partes do ciclo. Repetir o fluxo inteiro com dois clientes. |
| Layout e acessibilidade | Em janela estreita, operar tela, câmera e saída a 100%, 150% e 200%. Navegar por teclado e confirmar nome, estado e foco com Narrador no Windows e Orca no Linux. | Escala interna de 150% e 200% e árvore de acessibilidade verificadas. Falta leitura por voz e teste com escala do sistema em hardware real. |
| Distribuição | Gerar instalador Windows e AppImage Linux, instalar em perfis limpos, testar atualização de 0.9.x, verificar assinatura ou hash publicado e conferir que o servidor foi construído com a correção de negociação de codecs. | Executar nos artefatos candidatos antes de criar a tag 1.0. |

Para cada execução, anote **passou**, **falhou** ou **não executado**. Em caso
de falha, registre a ação que a reproduz, o resultado esperado, o observado e
apenas os trechos de log necessários, sem credenciais. Qualquer falha que
impeça entrar, assistir, parar uma transmissão ou atualizar o pacote bloqueia
o lançamento. Critérios pendentes precisam de evidência antes da versão final.

## Primeira execução do roteiro — 10/11/2026

Base do cliente: `3804c81`, versão `0.9.2`. O LiveKit foi construído e
executado em um projeto Docker local de validação, encerrado após os testes.

| Verificação | Resultado |
| --- | --- |
| Suítes automatizadas | Windows: 53 aprovados e 11 ignorados. Linux em WSL: 51 aprovados e 11 ignorados na repetição. Na primeira execução, o teste de encerramento durante o handshake WebSocket expirou antes de aceitar a conexão de teste. Passou isoladamente e na repetição completa. Python: 32 aprovados e 3 ignorados. |
| Interface do portátil Windows | Entrou na sala local, pausou e retomou o vídeo e abriu e cancelou o seletor de fonte em 150% e 200% de escala interna. Saiu da sala. O teste usou o executável da pasta de empacotamento. |
| Codecs e cotas do SFU | O teste com fontes geradas confirmou tela HEVC e câmera H.264 no mesmo publicador. Outro teste confirmou três telas e três câmeras geradas, recusou o sétimo vídeo e recebeu uma câmera na vaga liberada por uma tela. |
| Distribuição Windows | ZIP portátil e instalador gerados e conferidos contra o arquivo de SHA-256 local. A instalação e a atualização em perfil limpo ficam pendentes. O computador já tem uma instalação registrada. |
| Distribuição Linux | AppImage gerado e conferido contra o SHA-256 local. Abriu em WSL/X11 e entrou na sala local. Os quatro comandos da barra foram encontrados pelo AT-SPI. Esse build não inclui suporte NVIDIA no WebRTC, pois os cabeçalhos CUDA não estavam disponíveis. O teste em uma instalação física continua pendente. |
| Hardware e acessibilidade | Webcam física, três telas reais sob carga, leitura por voz e Linux físico com Wayland ficam pendentes nesta execução. |

## Instalação e atualização em perfil limpo — 10/11/2026

- **Windows Sandbox:** instalação limpa, abertura e saída da interface e
  desinstalação passaram. O executável instalado corresponde ao SHA-256 do
  candidato. Em seguida, o instalador oficial 0.9.1 foi aplicado e atualizado
  pelo instalador local 0.9.2. Nome, idioma e preferências mantiveram os mesmos
  hashes durante a atualização. A interface voltou a abrir e a desinstalação
  removeu o executável e o registro. A instalação do usuário no host permaneceu
  registrada. O teste executou a atualização pelo instalador, sem exercitar o
  download do atualizador integrado.
- **Linux em WSL/X11:** o AppImage candidato iniciou e encerrou em um perfil
  XDG vazio. Em um segundo perfil isolado, o AppImage oficial 0.9.1 iniciou
  primeiro. Após substituir o AppImage, o candidato 0.9.2 iniciou e preservou
  nome, idioma, FPS, volume, estado da lateral e modo de grade. Os testes usaram
  extração do AppImage e renderização por software, sem validar FUSE ou Wayland
  em uma instalação física.

## Validações complementares sem câmera — 10/11/2026

| Verificação | Resultado |
| --- | --- |
| Atualizador integrado | Em Windows Sandbox limpo, o 0.9.1 publicado encontrou o 0.9.2 publicado, baixou o instalador pela interface e reiniciou no executável atualizado. Nome, idioma, FPS, volume e grade foram preservados. O pacote candidato local foi validado separadamente pelo instalador, conforme a seção anterior. |
| Entrada e convites | O serviço de admissão, com banco temporário, rejeitou chave inválida com código 401. Duas sessões válidas criaram e encontraram a sala. Um cliente entrou por convite curto e o outro pelo catálogo, saiu e voltou a entrar. A rejeição foi verificada no serviço. A mensagem do formulário ainda precisa ser confirmada no cliente. |
| Reconexão | Dois clientes Windows do candidato transmitiram e receberam uma janela de teste em movimento com áudio. Uma interrupção de quatro segundos na sinalização do transmissor forçou a reconexão. O espectador preservou o último quadro e voltou a receber vídeo e áudio. |
| AppImage e FUSE | O candidato abriu e encerrou em perfil limpo com montagem FUSE, em Wayland no WSLg, com `DISPLAY` desativado. A captura via portal do sistema e o áudio em Linux físico ainda precisam ser testados. |
| Windows para Linux | O AppImage em WSLg/Wayland recebeu a janela de teste transmitida pelo cliente Windows. O diagnóstico registrou 1.629 quadros recebidos, duas texturas e 1.357 unidades de áudio recebidas. |
| Linux para Windows | O AppImage em WSL/X11 iniciou o compartilhamento de um desktop virtual pelo menu acessado via AT-SPI. O cliente Windows recebeu 581 quadros e apresentou 575 pela GPU. O áudio no sentido Linux para Windows continua pendente. |
| Menu sobre a transmissão | No Windows, a janela de vídeo nativa cobria parte do menu Mais opções. No build de depuração com a correção, dois clientes reproduziram uma transmissão em movimento. O menu permaneceu legível, o comando de pausa respondeu e o vídeo voltou a ocupar a área inteira ao retomar. |

## Chave, três transmissões e escala no Windows — 10/11/2026

| Verificação | Resultado |
| --- | --- |
| Chave de acesso no formulário | Em um perfil isolado, o formulário exibiu “Informe uma chave de acesso válida” para uma chave curta e “Chave inválida, expirada ou revogada” para uma chave bem-formada rejeitada com HTTP 401. Corrigir a chave na mesma janela liberou o acesso à lista de salas. |
| Três compartilhamentos simultâneos | Três clientes Windows compartilharam janelas de teste distintas em HEVC pelo LiveKit local. Um quarto cliente assistiu às três no Grid, com texto e contadores em movimento. Uma quarta tentativa de publicar foi recusada pelo SFU e o participante permaneceu na sala. Pausar e retomar restaurou os três vídeos. O cenário usou janelas de teste no mesmo computador. |
| Escala do HyperCast | O primeiro teste visual encontrou cartões espremidos e textos sobrepostos a 200%. A grade agora usa mais colunas quando falta altura e reduz as margens dos cartões baixos. O reteste com três transmissões mostrou imagens e títulos separados a 150% e 200%. A escala do Windows foi verificada separadamente, mas o aplicativo não chegou aos DPI esperados nas tentativas, então esse teste continua pendente. |
| Narrador | A recapitulação do Narrador confirmou o nome e o papel de botão de Mais opções. Tab chegou ao botão. Não foi possível confirmar a leitura e a operação completas de Pausar, Retomar e Sair pelo Narrador. |
| Suíte Windows | 54 testes passaram e 11 foram ignorados. |
