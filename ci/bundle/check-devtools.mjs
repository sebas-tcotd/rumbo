// AC 7 of story 0.6: @rumbo/devtools must not ship in the publication build.
// Run after `pnpm turbo run build` (which builds apps/web/dist and apps/web/dist-lab):
//   node ci/bundle/check-devtools.mjs
import { readdirSync, readFileSync } from 'node:fs';
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

/** Returns the list of problems (empty when both builds are as expected). */
export function checkBuilds({ dist, distLab, marker }) {
  const problems = [];
  const js = (dir) => files(dir).filter((f) => f.endsWith('.js'));
  const withMarker = (dir) => js(dir).filter((f) => readFileSync(f, 'utf8').includes(marker));
  const publication = js(dist);
  if (publication.length === 0) problems.push(`${dist}: no JavaScript found (build first)`);
  for (const f of withMarker(dist)) problems.push(`publication build contains devtools: ${relative(dist, f)}`);
  for (const f of files(dist)) {
    if (/devtools/i.test(relative(dist, f))) problems.push(`publication build emits a devtools chunk: ${relative(dist, f)}`);
  }
  if (withMarker(distLab).length === 0) problems.push(`lab build is missing devtools: no file in ${distLab} has the marker`);
  return problems;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const marker = readMarker(readFileSync(join(repo, 'packages/devtools/mount-devtools.tsx'), 'utf8'));
  const problems = checkBuilds({
    dist: join(repo, 'apps/web/dist'),
    distLab: join(repo, 'apps/web/dist-lab'),
    marker,
  });
  if (problems.length) {
    for (const p of problems) console.error('bundle-check: ' + p);
    process.exit(1);
  }
  console.log('bundle-check: OK (devtools absent from dist/, present in dist-lab/)');
}
