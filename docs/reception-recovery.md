# Recuperação automática da recepção

Incremento de desenvolvimento posterior à versão 0.9.0. O observador acompanha até três compartilhamentos, separadamente. Se o SDK comprovar chegada de quadros completos sem decode, ou decode sem entrega no leitor local, ele tenta recuperar apenas aquela transmissão. O último quadro continua visível.

A coleta acontece a cada cinco segundos, com timeout de três segundos e uma única tarefa em andamento. A falta de progresso precisa persistir por pelo menos oito segundos amostrados: não é uma ação imediata. Ausência de atualizações por si só não indica falha. Tela estática, pausa, faixa muda, falta de associação exata da estatística e reconexão suspendem a avaliação.

A primeira tentativa recria o leitor. As seguintes pausam o vídeo afetado por 700 ms antes de retomar conforme o layout atual, mantendo as demais transmissões e o áudio. Há até três ações por episódio e intervalo crescente entre elas. Quinze segundos de progresso amostrado contínuo repõem o orçamento. Alternar foco ou pausar não o repõe. Ao atingir o limite, use **Atualizar recepção**; sair da sala continua cancelando as tarefas.

O cartão informa quando está recuperando ou quando é preciso atualizar manualmente. **Debug → Copiar relatório técnico** inclui contadores de ações e motivos, sem nomes, faixas, endereços ou conteúdo de tela. A recuperação não mede latência fim a fim e não intercepta falhas nativas de driver.

## Validação reproduzível

`reception_recovery::tests` verifica silêncio/padding, contadores reiniciados, associação temporal, falha de decode/leitor, três ações máximas, alternância de pausa/foco e reposição somente após progresso estável.

O caso ignorado `automatic_reception_recovers_only_stalled_stream` exige uma sala isolada com duas origens de vídeo/áudio sintéticos. O executor `python -m tests.integration_reception_recovery` prepara essa sala e remove-a ao terminar. Ele usa `.tools/homelab.env` local para tokens curtos; nenhuma credencial entra na saída ou no repositório. Não captura desktop ou áudio pessoal.

Para Windows, configure `HYPERCAST_RECOVERY_WINDOWS=1`; o executor copia o artefato de teste e as DLLs fixadas para uma pasta temporária. Para Linux, compile o artefato com `cargo test --locked --features desktop --lib --message-format=json` em `.tools/pipewire-frame-build.jsonl`. O executor usa o runtime do AppImage da versão do Cargo e a distribuição WSL `Ubuntu-24.04`, alterável por `HYPERCAST_WSL_DISTRO`.

A regressão injeta uma interrupção somente no leitor de uma faixa, verifica retenção do quadro, substituição do leitor, pausa/retomada exclusiva do vídeo na segunda falha, continuidade da outra faixa e áudio, ausência de tentativas durante pausa e liberação ao sair. Testes de política cobrem o limite de três ações; não há injeção de travamento de driver. A matriz física AMD/Bazzite permanece pendente.

## Resultado em 2026-10-19

- 38 testes unitários passaram no Windows e 34 no Linux; casos que exigem sala/hardware foram executados separadamente quando indicados abaixo.
- A regressão nativa HEVC com duas origens sintéticas passou nos dois sistemas: leitor substituído na primeira falha, vídeo individual pausado/retomado na segunda, outro vídeo e áudio avançando, pausa sem ações extras e saída limpa.
- Após sair, o teste confirmou zero superfícies D3D11 retidas no Windows e zero decoders de teste no Linux.
- A inicialização da regressão tolera amostras anteriores à negociação RTP. Ela só provoca a falha depois que as duas associações exatas estiverem utilizáveis; ausência temporária de estatística continua sem disparar recuperação.

Esses resultados validam o incremento no código. Não representam uma nova release/instalador nem teste físico no Bazzite ou em GPU AMD.
