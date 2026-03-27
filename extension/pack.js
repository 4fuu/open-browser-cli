import archiver from 'archiver';
import { createWriteStream, existsSync } from 'fs';
import { resolve } from 'path';

const distDir = resolve('dist');
const manifestFile = resolve('manifest.json');
const outFile = resolve('dist/browser-cli-extension.zip');

if (!existsSync(distDir)) {
  console.error('dist/ not found — run npm run build first');
  process.exit(1);
}

const output = createWriteStream(outFile);
const archive = archiver('zip', { zlib: { level: 9 } });

output.on('close', () => {
  console.log(`Packed ${archive.pointer()} bytes → dist/browser-cli-extension.zip`);
});

archive.on('error', (err) => { throw err; });

archive.pipe(output);
archive.file(manifestFile, { name: 'manifest.json' });
archive.directory(distDir, false, (entry) => entry.name.endsWith('.zip') ? false : entry);
archive.finalize();
