import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../..', import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@assets': fileURLToPath(new URL('../../assets', import.meta.url)),
    },
  },
  worker: { format: 'es' },
  server: { fs: { allow: [repo] } },
  build: { target: 'es2022' },
});
