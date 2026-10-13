# Atualizações pelo aplicativo — 0.5.1

Use **Atualizar**, no topo da lateral. Ele abre a janela e consulta a versão
estável mais recente do repositório oficial. **Verificar atualização** permite
repetir a consulta. A verificação é manual, sem download automático ao iniciar.

Na versão **instalada**, escolha **Baixar atualização**. O progresso aparece
sem bloquear a interface. Depois do download verificado, use **Instalar e
reiniciar**. O cliente encerra a sessão, executa o instalador por usuário no
mesmo diretório e reabre o HyperCast. O Windows não é reiniciado. Uma versão
nova disponível não interrompe automaticamente um compartilhamento ativo.

Na versão **portátil**, **Baixar no navegador** abre a página da versão no
navegador padrão do Windows, inclusive Chrome se estiver definido como padrão.
Extraia o ZIP novo em outra pasta. **Abrir página de downloads** fica disponível
mesmo sem atualização ou em caso de falha na consulta.

0.4.x não possui o atualizador: instale 0.5.1 manualmente uma vez. As versões
seguintes poderão ser atualizadas pelo botão. A detecção usa a associação do
executável ao diretório registrado pelo Inno Setup em HKCU; não depende da
existência de um `unins000.exe` próximo de um executável portátil.

## Distribuição e verificação

Releases estáveis de `LucasCamy/HyperCast` precisam conter
`HyperCast-<versão>-windows-x64-setup.exe` e o ZIP portátil. O GitHub fornece
tamanho e digest SHA-256 do asset na API. O cliente valida HTTPS, domínio,
repositório, versão estável, nome, tamanho máximo (256 MiB), tamanho efetivo e
hash. Redirecionamentos aceitam somente hosts GitHub explícitos. Não contém
API secret, convite, JWT nem token GitHub para consultar ou baixar atualizações.

Checksum verifica a integridade a partir da release confiada via HTTPS;
não substitui uma assinatura Authenticode. Após verificar o arquivo, o cliente
mantém um handle que bloqueia escrita/remoção até iniciar o instalador. Arquivos
temporários ficam em `%LOCALAPPDATA%/HyperCast/updates/<UUID>` e não são usados
para executar comandos de shell. Downloads antigos são limpos por allowlist,
sem exclusão recursiva e sem remover conteúdo desconhecido.

Publicação: gere os pacotes com `pwsh -File tools/package.ps1`, faça commit/push,
crie a release com tag `v<versão>` no commit correspondente e publique o setup,
ZIP e `SHA256SUMS.txt`. Sem release estável completa, o cliente mostra a
indisponibilidade; erros de rede/limite da API permitem tentar novamente.

`pwsh -File tools/publish-release.ps1 -NotesFile <arquivo.md>` valida os hashes,
exige checkout limpo, cria um draft com todos os assets e só então publica como
Latest. Teste operacional: `python -m tests.integration_app_update` usa uma
instalação registrada isolada, uma versão privada anterior do mesmo cliente e
o instalador oficial baixado do GitHub; recusa uma instalação de usuário existente.

Referência: [GitHub — release assets e digest](https://docs.github.com/en/rest/releases/assets).
