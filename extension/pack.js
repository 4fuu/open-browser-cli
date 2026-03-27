import archiver from 'archiver';
import { createWriteStream, existsSync } from 'fs';
import { resolve } from 'path';

const distDir = resolve('dist');
const manifestFile = resolve('manifest.json');

if (!existsSync(distDir)) {
  console.error('dist/ not found — run npm run build first');
  process.exit(1);
}

function pack(outFile) {
  return new Promise((res, rej) => {
    const output = createWriteStream(outFile);
    const archive = archiver('zip', { zlib: { level: 9 } });
    output.on('close', () => res(archive.pointer()));
    archive.on('error', rej);
    archive.pipe(output);
    archive.file(manifestFile, { name: 'manifest.json' });
    // exclude any previously generated zip/xpi from the dist dir
    archive.directory(distDir, false, (entry) =>
      /\.(zip|xpi)$/.test(entry.name) ? false : entry,
    );
    archive.finalize();
  });
}

const [zipBytes, xpiBytes] = await Promise.all([
  pack(resolve('dist/browser-cli-extension.zip')),
  pack(resolve('dist/browser-cli-extension.xpi')),
]);

console.log(`Packed ${zipBytes} bytes  → dist/browser-cli-extension.zip  (Chrome / ungoogled-chromium)`);
console.log(`Packed ${xpiBytes} bytes  → dist/browser-cli-extension.xpi  (Firefox)`);
