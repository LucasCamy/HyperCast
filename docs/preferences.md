# Preferências

Resolução de envio, FPS, perfil de upload, modo Manual/Automático, codec, volume, estado do menu e modo Grid/Destaque são restaurados ao abrir. No Windows também são preservados o perfil AMD e a opção de mostrar a própria prévia. Nenhum compartilhamento começa automaticamente.

- Windows: `%LOCALAPPDATA%/HyperCast/preferences.json`.
- Linux: `${XDG_STATE_HOME:-$HOME/.local/state}/hypercast/preferences.json`.

Nome continua em `display-name` e idioma em `language`. O idioma escolhido no instalador Windows permanece preservado nas atualizações silenciosas. Essas preferências não contêm chave de servidor, JWT, sala, endereço, monitor, janela selecionada ou autorização de Portal/PipeWire. Áudio de captura e backend de software não são restaurados automaticamente.

## Restauração segura

Codec salvo continua sujeito à consulta de capacidades do encoder. Incompatibilidade confirmada bloqueia o início do envio e mostra o motivo; dado desconhecido permite tentativa com erro recuperável. Não troca silenciosamente o codec ou a GPU. Perfil AMD só é aplicado no envio HEVC por uma origem AMD; NVIDIA/AV1 continuam com seu comportamento próprio. A preferência AMD é preservada mesmo ao usar outra origem. O Linux preserva esse campo sem aplicá-lo a GStreamer.

Campos opcionais `rate_mode`, `codec` e `amd_profile` pertencem ao esquema 1. Arquivos antigos sem esses campos mantêm as demais escolhas e usam Manual, HEVC e Fluidez. Valores desconhecidos/fora dos limites e JSON truncado usam padrões na sessão. Não sobrescrever um arquivo somente por abrir o app: a gravação requer mudança de opção ou restauração explícita.

Versão de esquema diferente, inclusive arquivos criados por outro processo depois de abrir o app, é preservada byte a byte. Arquivos maiores que 4096 bytes também são preservados, pois não podem ser inspecionados dentro do limite de leitura. Debug avisa que mudanças valem apenas na sessão; restaurar padrões também não apaga esse arquivo protegido. Para migrar uma versão mais nova, use uma versão compatível do app. Chaves adicionais desconhecidas do esquema 1 são conservadas ao atualizar os campos conhecidos, dentro do mesmo limite.

## Gravação e concorrência

Mudanças são gravadas por trabalhador fora da renderização, com coalescência, arquivo temporário exclusivo, sincronização e substituição atômica no mesmo diretório. Processos que implementam este protocolo usam `preferences.lock` para serializar a verificação da versão e a substituição. O arquivo de lock permanece no diretório; o bloqueio do sistema é liberado ao fechar o handle ou terminar o processo. Não removê-lo com instâncias abertas.

Instâncias concorrentes usam a última gravação completa; não há mesclagem campo a campo entre escolhas de janelas diferentes. O bloqueio tem espera limitada de um segundo. Se ocupado ou se o disco falhar, as escolhas continuam na sessão e Debug oferece **Tentar salvar novamente**. Versões antigas que não implementam o protocolo não participam dessa coordenação.

Ao fechar normalmente, a última alteração pendente é encaminhada ao trabalhador. Erros de disco continuam podendo impedir a gravação; término forçado pode perder alterações pendentes. A espera do lock é limitada, mas isso não estabelece prazo máximo para chamadas de I/O do sistema operacional.

## Voltar aos padrões

Fora da sala e sem conexão/saída pendente, abra **Debug → Restaurar preferências… → Restaurar padrões**. O app explica o escopo e permite cancelar antes de aplicar. Restaura envio, codec, perfil AMD, volume e visualização. Preserva nome, idioma, acesso ao servidor, sala e seleção de origem; não conecta nem compartilha automaticamente. Se a gravação falhar ou o arquivo for protegido, o estado padrão ainda funciona na sessão.

Também é possível fechar todas as instâncias e remover somente `preferences.json`. Isso não remove nome, idioma ou acesso protegido. O arquivo só será recriado quando uma opção mudar. Execuções smoke e testes unitários não usam as preferências reais; testes de arquivo/GUI usam diretórios isolados.
