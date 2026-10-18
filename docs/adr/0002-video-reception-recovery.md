# ADR 0002 — Recuperação automática da recepção

- Data: 2026-10-18
- Estado: Aceito; planejado
- Marco: 0.9.0

## Contexto e decisão

Decisão: observador por publicação, até três telas, distingue pausa, faixa muda, desconexão, ausência legítima de atualização e falta de progresso no decoder. Não tratar tela estática ou stream fora de foco como falha apenas por falta de quadros.

Recuperação escalonada: solicitar quadro-chave quando houver API suportada; reinstalar leitor; renovar subscrição somente se necessário. Último quadro fica retido com estado de recuperação. Limitar três ações por episódio com cooldown, cancelar ao sair e reiniciar orçamento somente após progresso estável. Ação manual permanece disponível. Publicação local e áudio das outras telas não devem ser interrompidos.

O SDK fixado não expõe solicitação explícita de keyframe no receptor; não inventar essa chamada nem fazer renegociação agressiva para simulá-la. Antes da implementação, avaliar API nativa e evidência de bytes RTP versus quadros decodificados.

Aceite: origem estática não dispara laço; pausa/minimização não dispara; uma faixa falha sem resetar todas; retomada apresenta quadro novo e mantém limites de memória.
