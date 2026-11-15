// Junta results-*.json de todos los entornos y valida la matriz completa (story 0.9): los seis entornos
// y los ocho replays por identidad, un resultado por caso y entorno con un hash por tick declarado, igual
// a la referencia nativa en cada tick. Escribe una tabla Markdown y sale con 1 ante cualquier problema.
//
// Uso: node summary.mjs <carpeta con results-*.json> [--reference reference/native.json] [--step-summary <archivo>]
// --step-summary agrega la tabla a ese archivo ($GITHUB_STEP_SUMMARY) sin tubería, así el código de
// salida del script es el del paso.
import { appendFileSync, existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { listCases } from './cases.mjs';
import { evaluateMatrix } from './evaluate.mjs';

const here = dirname(fileURLToPath(import.meta.url));

/** Reads every results-*.json under `dir` (artifacts land in one subfolder each). Unreadable files are problems. */
export function readEnvs(dir) {
  const problems = [];
  if (!existsSync(dir)) return { envs: [], problems: [`no existe la carpeta de resultados: ${dir}`] };
  const files = readdirSync(dir, { recursive: true })
    .map(String)
    .filter((f) => /results-[\w-]+\.json$/.test(f))
    .sort();
  const envs = [];
  for (const f of files) {
    try {
      envs.push(JSON.parse(readFileSync(join(dir, f), 'utf8')));
    } catch (e) {
      problems.push(`${f}: ${e.message}`);
    }
  }
  return { envs, problems };
}

function cell(c) {
  if (c.ok) return '✅';
  if (c.firstMismatch !== null) return `❌ tick ${c.firstMismatch}`;
  return c.problems[0] === 'falta el resultado' || c.problems[0] === 'falta el entorno' ? '—' : '❌';
}

/** Builds the Markdown report and the verdict. */
export function summarize({ dir, referencePath, root }) {
  const { envs, problems: readProblems } = readEnvs(dir);
  let reference;
  const problems = [...readProblems];
  try {
    reference = JSON.parse(readFileSync(referencePath, 'utf8'));
  } catch (e) {
    problems.push(`referencia nativa ilegible: ${e.message}`);
  }
  let fixtures = [];
  try {
    fixtures = listCases(root);
  } catch (e) {
    problems.push(`fixtures ilegibles: ${e.message}`);
  }
  const m = evaluateMatrix(envs, fixtures, reference);
  problems.push(...m.problems);
  const ticksBy = new Map(fixtures.map((f) => [f.name, f.ticks]));
  const lines = [
    '## Determinismo entre plataformas',
    '',
    'Hash canónico del estado comparado en **cada tick** con la referencia nativa.',
    '',
    `| Replay | Ticks | ${m.envs.join(' | ')} |`,
    `| --- | --- | ${m.envs.map(() => '---').join(' | ')} |`,
    ...m.cases.map((c) => `| ${c} | ${ticksBy.get(c) ?? '—'} | ${m.envs.map((e) => cell(m.cells[c][e])).join(' | ')} |`),
    '',
    ...envs.map((e) => `- **${e?.browser}**: ${e?.userAgent ?? '?'}${e?.browserVersion ? ` (${e.browserVersion})` : ''} · física ${e?.physics ?? '?'}`),
    '',
  ];
  if (problems.length) {
    lines.push(`**${problems.length} problemas:**`, '', ...problems.map((p) => `- ${p}`));
  } else {
    lines.push(`**Cero diferencias**: ${m.cases.length} replays idénticos a nativo en cada tick, en ${m.envs.length} entornos.`);
  }
  return { ok: problems.length === 0, markdown: lines.join('\n') };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = (k, d) => {
    const i = process.argv.indexOf(k);
    return i > 0 ? process.argv[i + 1] : d;
  };
  const dir = process.argv[2] && !process.argv[2].startsWith('--') ? process.argv[2] : 'results';
  const { ok, markdown } = summarize({
    dir,
    referencePath: resolve(arg('--reference', join(here, 'reference/native.json'))),
    root: resolve(here, '../..'),
  });
  console.log(markdown);
  const stepSummary = arg('--step-summary');
  if (stepSummary) appendFileSync(stepSummary, `${markdown}\n`);
  process.exit(ok ? 0 : 1);
}
