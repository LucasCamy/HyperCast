# Preferências — 0.9.0

Resolução de envio, FPS, perfil de upload, volume, estado do menu e modo Grid/Destaque são restaurados ao abrir. No Windows também é preservada a opção de mostrar a própria prévia. Nenhum compartilhamento começa automaticamente.

- Windows: `%LOCALAPPDATA%/HyperCast/preferences.json`.
- Linux: `${XDG_STATE_HOME:-$HOME/.local/state}/hypercast/preferences.json`.

O nome continua em `display-name` e o idioma em `language`, sem migração destrutiva. O idioma escolhido no instalador Windows permanece preservado nas atualizações silenciosas.

O JSON não contém chave de servidor, JWT, sala, endereço, monitor ou janela selecionada. Arquivo ausente, inválido, maior que 4096 bytes ou com versão de esquema desconhecida usa padrões seguros. Mudanças de opções são gravadas por um trabalhador fora da renderização, com coalescência de alterações e substituição atômica. Ao fechar normalmente, a última alteração pendente é gravada. Em término forçado, alterações ainda pendentes podem não ser salvas. Instâncias concorrentes usam a última gravação completa.

Para restaurar padrões, feche o aplicativo e remova somente `preferences.json`. Isso não remove nome, idioma ou acesso protegido. O arquivo só será recriado quando alguma opção mudar. Execuções smoke e testes automatizados não usam as preferências reais.

O incremento posterior à 0.9.0 acrescenta `rate_mode` (`manual`/`automatic`) ao esquema 1. O campo ausente mantém Manual e preserva as outras escolhas; valores desconhecidos são inválidos. A escolha só é aplicada antes de iniciar o compartilhamento.
