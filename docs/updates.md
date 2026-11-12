# Atualizações pelo aplicativo — 0.5.3

Os botões do painel usam a mesma largura e altura, com a ação principal
destacada e as ações secundárias alinhadas.

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

Para preparar uma release, atualize a versão em `Cargo.toml` e `Cargo.lock`,
adicione as notas em `docs/<versão>.md` e integre essas mudanças à `main`.
No GitHub, abra **Actions → Release client → Run workflow** e selecione
`main`. Com **Publish** desmarcado, o workflow executa os testes e disponibiliza
os pacotes como artefatos da execução. Com **Publish** marcado, ele também
publica a tag `v<versão>` e a release estável após todos os jobs passarem.

O workflow gera o instalador e o ZIP portátil para Windows e o AppImage
para Linux. O Windows valida instalação limpa, execução em linha de comando
e atualização da versão estável anterior. O Linux executa o AppImage no Xvfb,
testa vídeo sintético em HEVC e verifica a preservação do perfil após substituir
o AppImage anterior. Os hashes dos arquivos enviados são conferidos no rascunho
da release antes de marcá-la como Latest.

`pwsh -File tools/package.ps1` gera os pacotes Windows localmente.
O teste operacional `python -m tests.integration_app_update` usa uma
instalação registrada isolada, uma versão privada anterior do mesmo cliente e
o instalador oficial baixado do GitHub; recusa uma instalação de usuário existente.

Referência: [GitHub — release assets e digest](https://docs.github.com/en/rest/releases/assets).

O instalador oferece Português (Brasil) e Inglês. Após a instalação, salva
`pt-BR` ou `en-US` em `%LOCALAPPDATA%/HyperCast/language`; a aplicação lê esse
arquivo ao iniciar. Atualizações silenciosas preservam a preferência existente.
Uma instalação manual pode alterar o idioma escolhido. O arquivo permanece no
perfil após desinstalar, como as demais preferências não secretas.
No Linux, o caminho é `${XDG_STATE_HOME:-$HOME/.local/state}/hypercast/language`.
`HYPERCAST_LANGUAGE=pt-BR` ou `en-US` permite validar o idioma sem alterar a
preferência salva. Os nomes de usuários e salas são exibidos como foram criados.
