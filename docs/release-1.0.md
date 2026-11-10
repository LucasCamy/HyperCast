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
