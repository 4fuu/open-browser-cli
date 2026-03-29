import archiver from 'archiver';
import { createWriteStream, existsSync } from 'fs';
import { resolve } from 'path';

const distDir = resolve('dist');
const chromeDir = resolve('dist/chrome');
const firefoxDir = resolve('dist/firefox');

if (!existsSync(distDir)) {
  console.error('dist/ not found — run npm run build first');
  process.exit(1);
}

if (!existsSync(chromeDir) || !existsSync(firefoxDir)) {
  console.error('Browser bundles not found — run npm run build first');
  process.exit(1);
}

function pack(sourceDir, outFile) {
  return new Promise((res, rej) => {
    const output = createWriteStream(outFile);
    const archive = archiver('zip', { zlib: { level: 9 } });
    output.on('close', () => res(archive.pointer()));
    archive.on('error', rej);
    archive.pipe(output);
    archive.directory(sourceDir, false, (entry) =>
      /\.(zip|xpi)$/.test(entry.name) ? false : entry,
    );
    archive.finalize();
  });
}

const [zipBytes, xpiBytes] = await Promise.all([
  pack(chromeDir, resolve('dist/browser-cli-extension.zip')),
  pack(firefoxDir, resolve('dist/browser-cli-extension.xpi')),
]);

console.log(`Packed ${zipBytes} bytes  → dist/browser-cli-extension.zip  (Chrome)`);
console.log(`Packed ${xpiBytes} bytes  → dist/browser-cli-extension.xpi  (Firefox unsigned package)`);
