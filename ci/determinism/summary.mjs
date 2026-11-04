// Junta results-*.json de todos los entornos en una tabla Markdown (para el resumen de CI).
// Uso: node summary.mjs <carpeta con results-*.json>
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { listCases } from './cases.mjs';

const dir = process.argv[2] ?? 'results';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const files = readdirSync(dir, { recursive: true }).filter((f) => String(f).match(/results-[\w-]+\.json$/));
const envs = files.map((f) => JSON.parse(readFileSync(join(dir, String(f)), 'utf8'))).sort((a, b) => a.browser.localeCompare(b.browser));
const cases = listCases(root);
const lines = ['## Determinismo entre plataformas', '', `| Replay | Nativo | ${envs.map((e) => e.browser).join(' | ')} |`, `| --- | --- | ${envs.map(() => '---').join(' | ')} |`];
let bad = 0;
for (const c of cases) {
  const cells = envs.map((e) => {
    const r = e.results?.find((x) => x.case === c.name);
    if (!r) {
      bad++;
      return '—';
    }
    if (!r.ok) bad++;
    return r.ok ? `✅ ${r.ms} ms` : `❌ tick ${r.firstMismatch}`;
  });
  lines.push(`| ${c.name} | \`${c.expected}\` | ${cells.join(' | ')} |`);
}
lines.push('', ...envs.map((e) => `- **${e.browser}**: ${e.userAgent} · física ${e.physics ?? '?'}${e.error ? ` · error: ${e.error}` : ''}`));
lines.push('', bad ? `**${bad} diferencias o casos faltantes.**` : `**Cero diferencias**: ${cases.length} replays idénticos a nativo en ${envs.length} entornos.`);
console.log(lines.join('\n'));
process.exit(bad ? 1 : 0);
