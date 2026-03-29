import * as esbuild from 'esbuild';
import { mkdir, readFile, rm, writeFile } from 'fs/promises';

const common = {
  bundle: true,
  format: 'iife',
  target: 'es2022',
  sourcemap: true,
};

const manifestTemplate = JSON.parse(await readFile('manifest.json', 'utf8'));
const targets = [
  ['chrome', buildChromeManifest(manifestTemplate)],
  ['firefox', buildFirefoxManifest(manifestTemplate)],
];

await rm('dist', { recursive: true, force: true });

await Promise.all(
  targets.map(async ([browser, manifest]) => {
    const outDir = `dist/${browser}`;
    await mkdir(`${outDir}/background`, { recursive: true });
    await mkdir(`${outDir}/content`, { recursive: true });

    await Promise.all([
      esbuild.build({
        ...common,
        entryPoints: ['src/background/service-worker.ts'],
        outfile: `${outDir}/background/service-worker.js`,
      }),
      esbuild.build({
        ...common,
        entryPoints: ['src/content/content-script.ts'],
        outfile: `${outDir}/content/content-script.js`,
      }),
      writeFile(`${outDir}/manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`),
    ]);
  }),
);

console.log('Build complete');

function buildChromeManifest(template) {
  const manifest = structuredClone(template);
  manifest.background = {
    service_worker: 'background/service-worker.js',
  };
  manifest.content_scripts = updateContentScriptPaths(manifest.content_scripts);
  delete manifest.browser_specific_settings;
  return manifest;
}

function buildFirefoxManifest(template) {
  const manifest = structuredClone(template);
  manifest.background = {
    scripts: ['background/service-worker.js'],
  };
  manifest.content_scripts = updateContentScriptPaths(manifest.content_scripts);
  return manifest;
}

function updateContentScriptPaths(contentScripts) {
  return contentScripts.map((script) => ({
    ...script,
    js: script.js.map((path) => path.replace(/^dist\//, '')),
  }));
}
