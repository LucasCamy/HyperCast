# Validação visual de latência

O relatório técnico não mede latência fim a fim. RTT, decode e durações de CPU ajudam a localizar problemas, mas não podem ser somados para comprovar menos de 100 ms. Instantes monotônicos de computadores diferentes também não devem ser comparados diretamente.

## Referência local, sem sincronizar relógios

1. Use uma sala de teste e compartilhe apenas `latency-reference.html`, incluído na pasta `support` do pacote Windows e em `usr/share/hypercast/support` no AppImage extraído. A página funciona localmente, sem rede e sem gravação. Não compartilhe conteúdo pessoal durante o teste.
2. Coloque os monitores de origem e recepção no mesmo enquadramento de uma câmera. Se os computadores estiverem longe, use equipamentos/um método equivalente com referência temporal validada; um vídeo só do receptor não basta para esta comparação.
3. Configure e anote resolução, FPS solicitado, perfil 40/25/12 Mbps, Manual/Automático, codecs, GPUs, frequência dos dois monitores e uso de LAN, WAN, VPN ou TURN. Mantenha a página visível e a aba ativa, pois navegadores reduzem o ritmo em segundo plano.
4. Grave preferencialmente a 240 FPS ou mais, com exposição curta e algarismos legíveis. Para cada quadro de câmera legível, subtraia o número recebido do número mostrado na origem. O mesmo contador é transmitido como imagem, dispensando sincronização de relógios entre os PCs.
5. Recolha ao menos 30 amostras distribuídas por um minuto, sem escolher só as melhores. Registre mediana, p95 e máximo; guarde os dois números de cada leitura para revisão. Anote também congelamentos e amostras ilegíveis. Não trate quadros repetidos da câmera como observações independentes.
6. Copie **Debug → Copiar relatório técnico** em ambos os clientes durante a sessão e perto do problema, antes de sair da sala. Não envie chaves/JWT ou gravações com nomes/conteúdo pessoal. A exportação é manual; não existe envio automático.

Esse procedimento estima a idade visual do conteúdo recebido em relação ao conteúdo visível na origem. Há incerteza pela frequência de atualização da página/monitores, quantização da câmera e rolling shutter. A 240 FPS a câmera tem passos de aproximadamente 4,17 ms; uma referência a 120 Hz muda a cada aproximadamente 8,33 ms. Esses números não são uma correção fixa a subtrair do resultado. Apresente a incerteza e não proclame sub-100 ms quando ela permitir ultrapassar o limite.

Repita em conexão externa sem VPN, TURN quando necessário, movimento e carga de GPU representativa. A referência ajuda a medir atraso, mas não representa a complexidade de compressão de um jogo. Combine-a com relatórios de sessões reais para avaliar qualidade e estabilidade.

Nenhum resultado de bancada sub-100 ms foi comprovado pela implementação da instrumentação. O teste físico permanece uma etapa explícita de validação.
