import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const repo = fileURLToPath(new URL('../..', import.meta.url));

/**
 * Devtools are a build-time feature flag: on in `vite dev`, in `vite build --mode lab` (output
 * `dist-lab/`) and with RUMBO_DEVTOOLS=1; off in the publication build (`dist/`), where the dynamic
 * `import('@rumbo/devtools')` sits behind a constant `false` and no chunk is emitted.
 */
export default defineConfig(({ command, mode }) => {
  const lab = mode === 'lab';
  const devtools = command === 'serve' || lab || process.env.RUMBO_DEVTOOLS === '1';
  return {
    plugins: [react()],
    resolve: {
      alias: {
        '@assets': fileURLToPath(new URL('../../assets', import.meta.url)),
      },
      // Two React roots (ui and devtools) on purpose, but one React copy.
      dedupe: ['react', 'react-dom'],
    },
    define: { 'import.meta.env.RUMBO_DEVTOOLS': JSON.stringify(devtools) },
    worker: { format: 'es' },
    server: { fs: { allow: [repo] } },
    build: { target: 'es2022', outDir: lab ? 'dist-lab' : 'dist', emptyOutDir: true },
  };
});
