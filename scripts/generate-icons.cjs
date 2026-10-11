// Development-only asset generation. The desktop app does not require Node.js.
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const sharp = require('sharp');

const directory = path.resolve(__dirname, '../assets/branding');
const pngSizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 192, 256, 512, 1024];
const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const hash = data => crypto.createHash('sha256').update(data).digest('hex');

async function main() {
  const source = await fs.readFile(path.join(directory, 'rastera.svg'));
  const frames = new Map();
  const files = [{ file: 'rastera.svg', sha256: hash(source) }];
  for (const size of pngSizes) {
    const data = await sharp(source, { density: 144 })
      .resize(size, size)
      .ensureAlpha()
      .png({ compressionLevel: 9 })
      .toBuffer();
    const file = `rastera-${size}.png`;
    await fs.writeFile(path.join(directory, file), data);
    frames.set(size, data);
    files.push({ file, width: size, height: size, sha256: hash(data) });
  }

  // Windows 10/11 supports PNG-compressed ICO frames, including transparency.
  const header = Buffer.alloc(6 + icoSizes.length * 16);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(icoSizes.length, 4);
  let offset = header.length;
  for (const [index, size] of icoSizes.entries()) {
    const frame = frames.get(size);
    const entry = 6 + index * 16;
    header[entry] = size === 256 ? 0 : size;
    header[entry + 1] = size === 256 ? 0 : size;
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(frame.length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += frame.length;
  }
  const ico = Buffer.concat([header, ...icoSizes.map(size => frames.get(size))]);
  await fs.writeFile(path.join(directory, 'rastera.ico'), ico);
  files.push({ file: 'rastera.ico', sizes: icoSizes, sha256: hash(ico) });
  const manifest = { source: 'rastera.svg', renderer: `sharp ${sharp.versions.sharp}`, files };
  await fs.writeFile(path.join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  console.log(`Generated ${pngSizes.length} PNGs and ICO with ${icoSizes.length} sizes from rastera.svg.`);
}

main().catch(error => { console.error(error); process.exitCode = 1; });
