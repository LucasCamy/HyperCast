# ADR 0001 — Recuperação e limites de falha

- Data: 2026-10-18
- Estado: Aceito; primeira etapa em implementação
- Marco: 0.9.0

## Contexto e decisão

Conexões, captura, codificação, recepção e interface têm ciclos de vida diferentes. Uma falha operacional em um deles não deve encerrar automaticamente todos os outros.

Decisão: usar Result para falhas esperadas; encerrar e liberar apenas o componente afetado. Conexão inicial com erro transitório terá no máximo três tentativas no total, com espera crescente e cancelável. Falhas de autenticação, permissão e configuração exigem ação do usuário. O SDK continua responsável pelas três tentativas da conexão já estabelecida; não criar dois reconectores concorrentes. Ao esgotar tentativas, apresentar ação manual reutilizável sem reiniciar o aplicativo.

Uma barreira catch_unwind no trabalhador de mídia contém panics Rust com unwind, descarta a sessão e permite recriá-la. Não continuar usando encoder/captura parcialmente alterados. Estado compartilhado com a UI deve ser substituído pelo padrão se envenenado, pois contém somente dados de apresentação. Nunca aplicar isso aos locks internos do SDK ou recursos nativos.

Segfault, access violation, abort, OOM e travamento em driver não são recuperáveis por catch_unwind. Isolar captura/codecs em processo supervisionado é uma etapa posterior, condicionada a protocolo e orçamento de cópias/latência. Não prometer recuperação dessas falhas no processo atual.

Aceite: cancelamento durante conexão/espera; credencial rejeitada sem repetição automática; falha de captura preserva recepção/sala; panic do trabalhador deixa UI utilizável; tentar novamente cria sessão nova; fechar tem prioridade. Testes de falha não devem capturar conteúdo pessoal.

## Primeira etapa no código

Implementados `src/resilience.rs` (limites do trabalhador, estado de apresentação descartável e espera de retry), conexão inicial cancelável com classificação de falha transitória e ação manual de nova tentativa em ambas as interfaces. No Linux, JoinError da seleção de captura e falha de envio são tratados localmente; no Windows, erro de reconfiguração após reconectar encerra apenas o compartilhamento.

Ainda pendentes: auditoria completa das esperas nativas/encerramento, isolamento em processo dos backends que precisarem e matriz prolongada em GPUs físicas. A existência desta barreira não permite afirmar que qualquer travamento nativo foi resolvido.
