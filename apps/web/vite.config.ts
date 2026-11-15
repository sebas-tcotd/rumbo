import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const repo = fileURLToPath(new URL('../..', import.meta.url));

function buildId(): string {
  if (process.env.GITHUB_SHA) return process.env.GITHUB_SHA.slice(0, 12);
  try {
    return execFileSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: repo, stdio: ['ignore', 'pipe', 'ignore'] })
      .toString()
      .trim();
  } catch {
    return 'dev';
  }
}

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
    define: {
      'import.meta.env.RUMBO_DEVTOOLS': JSON.stringify(devtools),
      'import.meta.env.RUMBO_BUILD': JSON.stringify(buildId()),
    },
    worker: { format: 'es' },
    server: { fs: { allow: [repo] } },
    // Safari 15.4 can run the capability checks and display the minimum-version message.
    build: { target: ['es2022', 'safari15.4'], outDir: lab ? 'dist-lab' : 'dist', emptyOutDir: true },
  };
});
