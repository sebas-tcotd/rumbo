// AC 7 of story 0.6: @rumbo/devtools must not ship in the publication build.
// Run after `pnpm turbo run build` (which builds apps/web/dist and apps/web/dist-lab):
//   node ci/bundle/check-devtools.mjs
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));

/** The runtime marker exported by @rumbo/devtools, read from its source so the two never drift. */
export function readMarker(source) {
  const marker = /export const DEVTOOLS_MARKER = '([^']+)'/.exec(source)?.[1];
  if (!marker) throw new Error('DEVTOOLS_MARKER not found in @rumbo/devtools');
  return marker;
}

function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(dir, e.name)) : [join(dir, e.name)],
  );
}

/** Minimum length of a JSX text run used as a fingerprint (short words appear anywhere). */
const MIN_TEXT = 16;

/**
 * Fingerprints of every devtools module, not only the one exporting the marker: the literal JSX texts
 * of its components and the class selectors of its stylesheet, minus anything the shipped code (ui and
 * the app) also contains. A partial leak (one panel imported statically) still trips the check.
 */
export function devtoolsFingerprints({ devtoolsSources, devtoolsCss, shippedSources, shippedCss }) {
  const shipped = shippedSources.join('\n');
  const texts = new Set();
  for (const source of devtoolsSources) {
    for (const [, run] of source.matchAll(/>([^<>{}]+)</g)) {
      const text = run.replace(/\s+/g, ' ').trim();
      // Prose only: TypeScript generics (`useState<X>(…)`) also sit between angle brackets.
      if (!/^[\p{L}\d .,·…'"¿?¡!/()-]+$/u.test(text)) continue;
      if (text.length >= MIN_TEXT && !shipped.includes(text)) texts.add(text);
    }
  }
  const selectors = (css) => new Set([...css.matchAll(/\.([a-zA-Z][\w-]*)/g)].map((m) => m[1]));
  const shippedClasses = selectors(shippedCss.join('\n'));
  const classes = [...selectors(devtoolsCss.join('\n'))].filter((c) => !shippedClasses.has(c));
  return { texts: [...texts], classes };
}

function leaks(dist, { marker, fingerprints }) {
  const problems = [];
  const all = files(dist);
  const js = all.filter((f) => f.endsWith('.js'));
  if (js.length === 0) problems.push(`${dist}: no JavaScript found (build first)`);
  for (const f of js) {
    const text = readFileSync(f, 'utf8');
    if (text.includes(marker)) problems.push(`publication build contains devtools: ${relative(dist, f)}`);
    const found = fingerprints.texts.find((t) => text.includes(t));
    if (found) problems.push(`publication build contains devtools text "${found}": ${relative(dist, f)}`);
  }
  for (const f of all.filter((f) => f.endsWith('.css'))) {
    const css = readFileSync(f, 'utf8');
    const found = fingerprints.classes.find((c) => new RegExp(`\\.${c}(?![\\w-])`).test(css));
    if (found) problems.push(`publication build contains devtools styles (.${found}): ${relative(dist, f)}`);
  }
  for (const f of all) {
    if (/devtools/i.test(relative(dist, f))) problems.push(`publication build emits a devtools chunk: ${relative(dist, f)}`);
  }
  return problems;
}

/** Returns the list of problems (empty when both builds are as expected). */
export function checkBuilds({ dist, distLab, marker, fingerprints = { texts: [], classes: [] } }) {
  for (const dir of [dist, distLab]) {
    if (!existsSync(dir)) return [`${dir}: missing (build first)`];
  }
  const problems = leaks(dist, { marker, fingerprints });
  const labHas = files(distLab).some((f) => f.endsWith('.js') && readFileSync(f, 'utf8').includes(marker));
  if (!labHas) problems.push(`lab build is missing devtools: no file in ${distLab} has the marker`);
  return problems;
}

/** Source files of a package folder with one of the extensions (tests excluded). */
function sources(dir, extensions) {
  return files(join(repo, dir))
    .filter((f) => extensions.some((e) => f.endsWith(e)) && !f.includes('node_modules') && !/[\\/]dist[\\/]/.test(f))
    .filter((f) => !/\.test\./.test(f))
    .map((f) => readFileSync(f, 'utf8'));
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const marker = readMarker(readFileSync(join(repo, 'packages/devtools/mount-devtools.tsx'), 'utf8'));
  const fingerprints = devtoolsFingerprints({
    devtoolsSources: sources('packages/devtools', ['.tsx']),
    devtoolsCss: sources('packages/devtools', ['.css']),
    shippedSources: [...sources('packages/ui', ['.tsx', '.ts', '.json']), ...sources('apps/web/src', ['.tsx', '.ts'])],
    shippedCss: [...sources('packages/ui', ['.css']), ...sources('apps/web/src', ['.css'])],
  });
  if (fingerprints.texts.length === 0 || fingerprints.classes.length === 0) {
    console.error('bundle-check: no devtools fingerprints found (texts or classes); the check would be vacuous');
    process.exit(1);
  }
  const problems = checkBuilds({
    dist: join(repo, 'apps/web/dist'),
    distLab: join(repo, 'apps/web/dist-lab'),
    marker,
    fingerprints,
  });
  if (problems.length) {
    for (const p of problems) console.error('bundle-check: ' + p);
    process.exit(1);
  }
  console.log(
    `bundle-check: OK (devtools absent from dist/, present in dist-lab/; ` +
      `${fingerprints.texts.length} texts and ${fingerprints.classes.length} classes checked)`,
  );
}
