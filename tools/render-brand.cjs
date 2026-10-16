// Deterministic exports from the SVG. Install the pinned renderer once with:
// npm install --prefix .tools/branding --no-audit --no-fund @resvg/resvg-js@2.6.2
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const { Resvg } = require(path.join(root, '.tools/branding/node_modules/@resvg/resvg-js'));
const svg = fs.readFileSync(path.join(root, 'assets/hypercast.svg'));
const render = size => Buffer.from(new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng());
fs.writeFileSync(path.join(root, 'assets/hypercast.png'), render(512));
const sizes = [16, 24, 32, 48, 64, 128, 256];
const images = sizes.map(render);
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(1, 2); // ICO, containing PNG-compressed RGBA images.
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
sizes.forEach((size, index) => {
  const entry = 6 + 16 * index;
  header[entry] = header[entry + 1] = size === 256 ? 0 : size;
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(images[index].length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += images[index].length;
});
fs.writeFileSync(path.join(root, 'assets/hypercast.ico'), Buffer.concat([header, ...images]));
console.log('Exported 512px PNG and seven native ICO sizes from hypercast.svg');
