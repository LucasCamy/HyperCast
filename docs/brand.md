# Identidade visual

O monograma mantém o verde `#bfef79` sobre fundo `#151b1c`. As duas hastes têm
terminações chanfradas e uma ponte ascendente, sugerindo ligação e transmissão.
A forma é sólida e sem detalhes finos para permanecer legível em ícones pequenos.

`assets/hypercast.svg` é a origem editável. O PNG é usado pelo aplicativo e
AppImage; o ICO contém desenhos renderizados diretamente em 16, 24, 32, 48, 64,
128 e 256 pixels para o executável e instalador Windows.

Para exportar novamente, na raiz do projeto:

```sh
npm install --prefix .tools/branding --no-audit --no-fund @resvg/resvg-js@2.6.2
node tools/render-brand.cjs
```

Os cabeçalhos Windows e Linux usam a mesma imagem por meio de `brand_mark`, com
cache da textura no contexto egui. O monograma não é uma letra dependente da
fonte instalada. Não há animação nem decodificação PNG a cada quadro.

## Revisão e evolução

1. **Diagnóstico Linux:** registrar as etapas portal, abertura PipeWire, negociação
   e primeiro quadro. Um erro Rust pode virar mensagem na interface, mas uma
   falha nativa pode encerrar o processo; o log do PC afetado é necessário antes
   de afirmar a causa. Considerar isolar captura/codificação em processo separado
   se esse tipo de falha for confirmado.
2. **Consistência entre sistemas:** compartilhar mais componentes além do logotipo
   e dos campos de texto. Os cabeçalhos, acesso à atualização e exposição do
   diagnóstico ainda têm implementações diferentes.
3. **Qualidade legível:** apresentar FPS real, qualidade do envio e estado de
   reconexão com linguagem simples, mantendo QP, VBV, NACK e detalhes técnicos no
   Debug. A média de QP não identifica sozinha a queda nos quadros-chave.

Na versão 0.8.6, botões principais, volume, saída da tela cheia e molduras de
Diagnóstico/Atualização compartilham componentes. O Linux também organiza o
formato de vídeo em seção recolhível. A barra comum da sala mostra resolução,
FPS/bitrate do envio e total de quadros decodificados na recepção. Veja as
[definições das medições](video-quality.md#indicadores-na-interface).
O diagnóstico do encerramento de captura no Bazzite permanece pendente do log.
