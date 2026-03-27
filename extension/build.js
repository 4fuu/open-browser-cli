import * as esbuild from 'esbuild';

const common = {
  bundle: true,
  format: 'esm',
  target: 'es2022',
  sourcemap: true,
};

await Promise.all([
  esbuild.build({
    ...common,
    entryPoints: ['src/background/service-worker.ts'],
    outfile: 'dist/background/service-worker.js',
  }),
  esbuild.build({
    ...common,
    entryPoints: ['src/content/content-script.ts'],
    outfile: 'dist/content/content-script.js',
  }),
]);

console.log('Build complete');
