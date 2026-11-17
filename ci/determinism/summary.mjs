// Junta results-*.json de todos los entornos y valida la matriz completa (story 0.9): los seis entornos
// y los ocho replays por identidad, un resultado por caso y entorno con un hash por tick declarado, igual
// a la referencia nativa en cada tick. Escribe una tabla Markdown y sale con 1 ante cualquier problema.
//
// Uso: node summary.mjs <carpeta con resultados> [--artifacts] [--reference reference/native.json] [--step-summary <archivo>]
// --artifacts exige un subdirectorio results-<entorno> por cada artefacto descargado en CI.
// --step-summary agrega la tabla a ese archivo ($GITHUB_STEP_SUMMARY) sin tubería, así el código de
// salida del script es el del paso.
import { appendFileSync, existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { listCases } from './cases.mjs';
import { evaluateMatrix, REQUIRED_ENVS } from './evaluate.mjs';

const here = dirname(fileURLToPath(import.meta.url));

/** Reads flat local reports or one report per downloaded artifact directory. */
export function readEnvs(dir, artifacts = false) {
  const problems = [];
  if (!existsSync(dir)) return { envs: [], problems: [`no existe la carpeta de resultados: ${dir}`] };
  const entries = readdirSync(dir, { withFileTypes: true }).filter((entry) => entry.name.startsWith('results-'));
  const artifactLayout = artifacts || entries.some((entry) => entry.isDirectory());
  const names = new Map(REQUIRED_ENVS.map((env) => [artifactLayout ? `results-${env}` : `results-${env}.json`, env]));
  const envs = [];
  const seen = new Set();
  for (const entry of entries) {
    const env = names.get(entry.name);
    if (!env || (artifactLayout ? !entry.isDirectory() : !entry.isFile())) {
      problems.push(`${artifactLayout ? 'artefacto' : 'resultado'} inesperado: ${entry.name}`);
      continue;
    }
    seen.add(entry.name);
    const filename = `results-${env}.json`;
    const file = artifactLayout ? join(entry.name, filename) : filename;
    try {
      if (artifactLayout) {
        const contents = readdirSync(join(dir, entry.name), { withFileTypes: true });
        if (contents.length !== 1 || contents[0].name !== filename || !contents[0].isFile()) {
          problems.push(`${entry.name}: se esperaba únicamente ${filename}`);
          continue;
        }
      }
      const report = JSON.parse(readFileSync(join(dir, file), 'utf8'));
      if (report?.browser !== env) {
        problems.push(`${file}: entorno declarado ${report?.browser ?? 'ausente'}, se esperaba ${env}`);
        continue;
      }
      envs.push(report);
    } catch (e) {
      problems.push(`${file}: ${e.message}`);
    }
  }
  if (artifactLayout) {
    for (const name of names.keys()) if (!seen.has(name)) problems.push(`falta el artefacto ${name}`);
  }
  return { envs, problems };
}

function cell(c) {
  if (c.ok) return '✅';
  if (c.firstMismatch !== null) return `❌ tick ${c.firstMismatch}`;
  return c.problems[0] === 'falta el resultado' || c.problems[0] === 'falta el entorno' ? '—' : '❌';
}

/** Builds the Markdown report and the verdict. */
export function summarize({ dir, referencePath, root, artifacts }) {
  const { envs, problems: readProblems } = readEnvs(dir, artifacts);
  let reference;
  const problems = [...readProblems];
  try {
    reference = JSON.parse(readFileSync(referencePath, 'utf8'));
  } catch (e) {
    problems.push(`referencia nativa ilegible: ${e.message}`);
  }
  let fixtures = [];
  try {
    fixtures = listCases(root, reference);
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
    artifacts: process.argv.includes('--artifacts'),
  });
  console.log(markdown);
  const stepSummary = arg('--step-summary');
  if (stepSummary) appendFileSync(stepSummary, `${markdown}\n`);
  process.exit(ok ? 0 : 1);
}
