# ADR 0006 — Preferências persistentes e restauração segura

- Data: 2026-10-18
- Estado: Aceito; preferências mínimas implementadas
- Marco: 0.9.0

## Contexto e decisão

Decisão: salvar configurações não secretas por usuário: idioma, nome, resolução/FPS, perfil, volume, menu recolhido e prévia própria. Formato versionado, leitura limitada, validação e escrita atômica. Reutilizar os locais de dados já usados pelo aplicativo e migrar preferências existentes.

Não persistir tokens de sala nesse arquivo. Credenciais continuam no armazenamento protegido da plataforma. Origem de captura só pode ser restaurada se ainda existir e a plataforma permitir; nunca contornar consentimento do portal. Não iniciar compartilhamento automaticamente ao abrir o aplicativo.

Aceite: round-trip, arquivo truncado/corrompido, valores de versão futura, dois processos e migração sem perder idioma/nome.

## Entrega mínima da 0.9.0

[Preferências](../preferences.md) de envio, volume, menu e modo de visualização. Nome e idioma continuam nos arquivos existentes; a preservação deles não exige reuni-los no JSON. Codec, perfil específico AMD, monitor/janela e áudio de captura não são restaurados automaticamente. Nenhuma fonte é aberta ao iniciar.
