# ADR 0006 — Preferências persistentes e restauração segura

- Data: 2026-10-18
- Estado: Aceito; persistência e restauração segura implementadas
- Marco: 0.9.0 e incremento posterior

## Contexto e decisão

Decisão: salvar configurações não secretas por usuário: idioma, nome, resolução/FPS, perfil, volume, menu recolhido e prévia própria. Formato versionado, leitura limitada, validação e escrita atômica. Reutilizar os locais de dados já usados pelo aplicativo e migrar preferências existentes.

Não persistir tokens de sala nesse arquivo. Credenciais continuam no armazenamento protegido da plataforma. Origem de captura só pode ser restaurada se ainda existir e a plataforma permitir; nunca contornar consentimento do portal. Não iniciar compartilhamento automaticamente ao abrir o aplicativo.

Aceite: round-trip, arquivo truncado/corrompido, valores de versão futura, dois processos e migração sem perder idioma/nome.

## Entrega mínima da 0.9.0

[Preferências](../preferences.md) de envio, volume, menu e modo de visualização. Nome e idioma continuam nos arquivos existentes; a preservação deles não exige reuni-los no JSON. Codec, perfil específico AMD, monitor/janela e áudio de captura não são restaurados automaticamente. Nenhuma fonte é aberta ao iniciar.

## Incremento posterior — 2026-10-20

Esquema 1 ampliado com codec e perfil AMD opcionais. Arquivos antigos recebem padrões só para campos ausentes, preservando as escolhas anteriores. Codec restaurado respeita a consulta de capacidades; perfil AMD só tem efeito na origem AMD/HEVC. Monitor, janela, áudio e backend de software continuam exigindo seleção explícita; não persistir autorização do portal nem iniciar envio ao abrir.

Gravações de processos cooperantes são serializadas por lock do sistema com espera limitada de um segundo, fora da pintura. A versão é verificada novamente sob lock antes da substituição atômica. Arquivo de versão desconhecida ou acima do limite de leitura permanece intacto, inclusive diante de restauração explícita. Campos adicionais do esquema 1 são preservados. Último snapshot completo vence; não há fusão de preferências entre processos nem garantia para versões antigas que não usam o lock.

Debug oferece restauração com confirmação local e explicação do que muda, disponível fora de salas/conexões/transições. Nome, idioma e acesso são preservados. Falha de gravação mantém as opções em memória, mostra aviso e permite tentar salvar novamente.

Critérios cobertos por testes: round-trip com codec/perfil, migração de campos ausentes, truncamento e limites, versão futura criada depois do início, restauração sem perda dos arquivos de nome/idioma, erro de I/O com nova tentativa e dois processos concorrentes observando somente snapshots completos. Ver [operação](../preferences.md). A validação de hardware AMD/Bazzite permanece no ADR 0005, sem ser inferida destes testes de preferências.

## Validação do incremento

- Windows: 49 testes unitários passaram. Linux: 46 testes unitários passaram, incluindo os testes de interface existentes para erro de conexão sem bloquear a pintura.
- Duas instâncias de teste gravaram snapshots em diretório isolado, com leituras concorrentes sem JSON parcial. Contenção além do prazo é recuperável por nova tentativa; o teste também respeita essa política.
- Falha de I/O provocada manteve escolhas em memória e a nova tentativa gravou após corrigir a causa. Versão futura inserida depois do início permaneceu intacta, inclusive ao restaurar padrões.
- GUI Windows em perfil isolado restaurou AV1 e escolhas de envio/volume existentes. A restauração explícita retornou para HEVC, volume 80% e Destaque; nome foi preservado. Outra execução mostrou o aviso de arquivo incompatível sem alterar seus bytes e manteve o idioma inglês do arquivo existente.

Este incremento foi versionado sem gerar nova release ou instalador. Não amplia as garantias de desempenho de mídia.
