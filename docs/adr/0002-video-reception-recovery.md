# ADR 0002 — Recuperação automática da recepção

- Data: 2026-10-18
- Estado: Aceito; primeiro incremento conservador implementado
- Marco: incremento posterior à 0.9.0

## Contexto e decisão

Decisão: observador por publicação, até três telas, distingue pausa, faixa muda, desconexão, ausência legítima de atualização e falta de progresso no decoder. Não tratar tela estática ou stream fora de foco como falha apenas por falta de quadros.

Recuperação escalonada: solicitar quadro-chave quando houver API suportada; reinstalar leitor; renovar subscrição somente se necessário. Último quadro fica retido com estado de recuperação. Limitar três ações por episódio com cooldown, cancelar ao sair e reiniciar orçamento somente após progresso estável. Ação manual permanece disponível. Publicação local e áudio das outras telas não devem ser interrompidos.

O SDK fixado não expõe solicitação explícita de keyframe no receptor; não inventar essa chamada nem fazer renegociação agressiva para simulá-la. Antes da implementação, avaliar API nativa e evidência de bytes RTP versus quadros decodificados.

Aceite: origem estática não dispara laço; pausa/minimização não dispara; uma faixa falha sem resetar todas; retomada apresenta quadro novo e mantém limites de memória.

## Primeiro incremento — 2026-10-19

Observador comum aos clientes Windows/Linux, por publicação, até três telas. Uma única coleta assíncrona de estatísticas a cada cinco segundos substitui a coleta agregada anterior. Cada coleta tem timeout de três segundos. A associação exige o identificador exato da faixa RTC; associação ausente ou ambígua suspende a detecção, sem escolher outra faixa por aproximação.

Silêncio RTP, padding, tela estática, faixa muda, pausa, origem fora de foco e reconexão não são evidência suficiente. O detector exige quadros completos recebidos sem decode, ou decode avançando sem entrega no leitor, durante ao menos oito segundos amostrados. Contadores reiniciados, falhas de coleta ou intervalos superiores a dez segundos interrompem a evidência acumulada.

Primeira ação: substituir somente o leitor afetado. Segunda e terceira: desabilitar somente o vídeo dessa publicação por 700 ms e reabilitar conforme o layout atual, com leitor novo. Não há unsubscribe, renegociação global ou promessa de pedido explícito de keyframe. Essa API não existe no SDK fixado. O áudio segue sua política original, inclusive durante a pausa temporária do vídeo.

No máximo três ações por episódio, com intervalo crescente; alternar foco/pausa não repõe o orçamento. Ele só é reposto após quinze segundos de progresso amostrado contínuo, ou por reinício manual. Sair cancela a coleta; reconexão descarta a evidência anterior. O último quadro é mantido e o cartão informa recuperação/limite de tentativas. O botão de atualizar recepção continua disponível.

Contadores agregados e motivos enumerados entram no relatório técnico, sem IDs de faixas ou participantes. Ver [operação e validação](../reception-recovery.md).

Limites: a política de falha do decoder é testada com contadores controlados; a regressão de mídia provoca falha do leitor local. Isso não valida recuperação de driver travado/segfault nem substitui teste físico AMD/Bazzite. Falha sem evidência suficiente permanece para o comando manual. Não foi introduzida medição de latência fim a fim.
