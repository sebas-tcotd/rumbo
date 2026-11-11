// Descarga un extracto de OpenStreetMap con Overpass y lo archiva con fecha.
//
// Uso: node tools/data-fetch/fetch-osm.mjs [consulta.overpassql] [región]
//   por defecto: data/queries/lima-costa-verde.overpassql, región lima-costa-verde
//   otro servidor: OVERPASS_URL=https://overpass.kumi.systems/api/interpreter node tools/data-fetch/fetch-osm.mjs
//
// Datos © colaboradores de OpenStreetMap, licencia ODbL (https://www.openstreetmap.org/copyright).
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '../..');
const queryPath = process.argv[2] ?? join(root, 'data/queries/lima-costa-verde.overpassql');
const region = process.argv[3] ?? 'lima-costa-verde';
const url = process.env.OVERPASS_URL ?? 'https://overpass-api.de/api/interpreter';

const query = readFileSync(queryPath, 'utf8').replace(/^\s*\/\/.*$/gm, '');
console.log(`Consultando ${url} …`);
const res = await fetch(url, {
  method: 'POST',
  headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'User-Agent': 'rumbo-track-compiler/0.1' },
  body: new URLSearchParams({ data: query }),
});
if (!res.ok) {
  console.error(`Overpass respondió ${res.status}: ${(await res.text()).slice(0, 400)}`);
  console.error('Si es 429 o 504, espera un minuto o usa otro servidor con OVERPASS_URL.');
  process.exit(1);
}
const text = await res.text();
const doc = JSON.parse(text);
const nodes = doc.elements.filter((e) => e.type === 'node').length;
const ways = doc.elements.filter((e) => e.type === 'way').length;
const day = new Date().toISOString().slice(0, 10);
mkdirSync(join(root, 'data/raw'), { recursive: true });
const out = join(root, 'data/raw', `${region}-${day}.overpass.json`);
writeFileSync(out, text);
console.log(`${ways} vías, ${nodes} nodos, ${(text.length / 1e6).toFixed(1)} MB`);
console.log(`Base de OSM: ${doc.osm3s?.timestamp_osm_base ?? 'desconocida'}`);
console.log(`Guardado en ${out}`);
