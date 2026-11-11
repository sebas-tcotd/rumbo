import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Fail closed: every dependency reachable from the kernel must be approved.
const allowed = new Set(['track-format', 'serde', 'serde_core', 'serde_derive', 'serde_json', 'itoa', 'memchr', 'zmij', 'proc-macro2', 'quote', 'syn', 'unicode-ident', 'postcard', 'embedded-io', 'cobs', 'thiserror', 'thiserror-impl', 'blake3', 'arrayref', 'arrayvec', 'cc', 'find-msvc-tools', 'shlex', 'cfg-if', 'constant_time_eq', 'cpufeatures', 'libc']);
export function checkDependencies(metadata, rootPackage = 'track-format') {
  const packages = new Map(metadata.packages.map(p => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map(n => [n.id, n]));
  const kernel = metadata.packages.find(p => p.name === rootPackage);
  const policy = rootPackage === 'track-compiler-core' ? new Set([...allowed, rootPackage, 'libm']) : allowed;
  if (!kernel) throw new Error(rootPackage + ' missing');
  const cli = rootPackage === 'compiler-cli';
  const forbiddenCli = new Set(['sim-core', 'sim-wasm', 'rapier3d', 'wasm-bindgen', 'web-sys', 'js-sys']);
  const seen = new Set();
  function visit(id) {
    if (seen.has(id)) return;
    seen.add(id);
    const p = packages.get(id);
    if (!p || (cli ? forbiddenCli.has(p.name) : !policy.has(p.name))) throw new Error(`forbidden kernel dependency: ${p?.name ?? id}`);
    const n = nodes.get(id);
    if (!n) throw new Error(`missing resolution: ${id}`);
    for (const d of n.deps) {
      // Include build and normal edges; dev-only edges cannot hide production edges.
      if (!d.dep_kinds.some(k => k.kind !== 'dev')) continue;
      if (rootPackage === 'track-compiler-core' && id === kernel.id &&
          !['track-format', 'serde', 'serde_json', 'libm'].includes(packages.get(d.pkg)?.name)) {
        throw new Error('forbidden direct compiler dependency: ' + d.pkg);
      }
      visit(d.pkg);
    }
  }
  visit(kernel.id);
}
// Inspect Rust tokens, not text inside comments or literals. Keep delimiters so
// nested cfg_attr and grouped imports cannot hide a forbidden operation.
function rustTokens(source) {
  const tokens = [];
  let i = 0;
  while (i < source.length) {
    if (/\s/.test(source[i])) { i++; continue; }
    if (source.startsWith('//', i)) {
      const end = source.indexOf('\n', i);
      i = end < 0 ? source.length : end + 1;
      continue;
    }
    if (source.startsWith('/*', i)) {
      let depth = 1;
      i += 2;
      while (i < source.length && depth) {
        if (source.startsWith('/*', i)) { depth++; i += 2; }
        else if (source.startsWith('*/', i)) { depth--; i += 2; }
        else i++;
      }
      if (depth) throw new Error('unterminated Rust comment');
      continue;
    }
    const raw = /^(?:br|cr|r)(#*)"/.exec(source.slice(i));
    if (raw) {
      const endMarker = `"${raw[1]}`;
      const end = source.indexOf(endMarker, i + raw[0].length);
      if (end < 0) throw new Error('unterminated Rust raw string');
      tokens.push('<literal>');
      i = end + endMarker.length;
      continue;
    }
    // A character literal has a closing quote; a lifetime such as 'a does not.
    const character = /^(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\r\n])|[^'\\\r\n])'/u.exec(source.slice(i));
    if (character) { tokens.push('<literal>'); i += character[0].length; continue; }
    const string = /^(?:b|c)?"/.exec(source.slice(i));
    if (string) {
      i += string[0].length;
      while (i < source.length && source[i] !== '"') {
        if (source[i] === '\\') i++;
        i++;
      }
      if (i >= source.length) throw new Error('unterminated Rust string');
      i++;
      tokens.push('<literal>');
      continue;
    }
    const identifier = /^(?:r#)?[A-Za-z_][A-Za-z_0-9]*/.exec(source.slice(i));
    if (identifier) {
      tokens.push(identifier[0].replace(/^r#/, ''));
      i += identifier[0].length;
    } else if (source.startsWith('::', i)) { tokens.push('::'); i += 2; }
    else tokens.push(source[i++]);
  }
  return tokens;
}

// `allowedRuntime` lifts the ban on one of the runtime crates for a core crate that legitimately
// uses it (sim-core and rapier3d); randomness, wasm-bindgen and tiff stay forbidden everywhere.
export function checkSource(source, file = 'source', kernel = true, allowedRuntime = []) {
  const tokens = rustTokens(source);
  const forbiddenSerde = new Set(['flatten', 'skip_serializing_if']);
  // The compiler has no runtime I/O boundary. Reject whole modules so aliases
  // and grouped imports cannot hide standard streams or subprocess access.
  const forbiddenModules = new Set(kernel ? ['fs', 'net', 'time'] : ['fs', 'net', 'time', 'io', 'process']);
  for (let i = 0; i < tokens.length; i++) {
    if (kernel && tokens[i] === 'serde' && tokens[i + 1] === '(') {
      let depth = 1;
      for (let j = i + 2; j < tokens.length && depth; j++) {
        if (tokens[j] === '(') depth++;
        else if (tokens[j] === ')') depth--;
        else if (depth === 1 && forbiddenSerde.has(tokens[j])) throw new Error(`forbidden serde attribute: ${file}`);
      }
    }
    if (['rand', 'rapier3d', 'wasm_bindgen', 'tiff'].includes(tokens[i]) && !allowedRuntime.includes(tokens[i]) && tokens[i + 1] === '::') throw new Error(`forbidden kernel runtime: ${file}`);
    if (tokens[i] !== 'std') continue;
    // Reject aliasing the entire std namespace or importing all of it: either
    // would require resolving arbitrary aliases in later paths. Safe submodule
    // imports (e.g. std::fmt as formatting) remain available.
    if (tokens[i + 1] === 'as') throw new Error(`forbidden kernel runtime alias: ${file}`);
    if (tokens[i + 1] !== '::') continue;
    if (forbiddenModules.has(tokens[i + 2]) || tokens[i + 2] === '*') throw new Error(`forbidden kernel runtime: ${file}`);
    if (tokens[i + 2] === '{') {
      let depth = 1;
      for (let j = i + 3; j < tokens.length && depth; j++) {
        if (tokens[j] === '{') depth++;
        else if (tokens[j] === '}') depth--;
        else if (forbiddenModules.has(tokens[j]) || tokens[j] === '*' || (depth === 1 && tokens[j] === 'self')) throw new Error(`forbidden kernel runtime import: ${file}`);
      }
    }
  }
}
function sources(dir, kernel = true, allowedRuntime = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) sources(p, kernel, allowedRuntime);
    else if (e.name.endsWith('.rs')) checkSource(readFileSync(p, 'utf8'), p, kernel, allowedRuntime);
  }
}
export function check(root = resolve('.')) {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }));
  checkDependencies(metadata);
  sources(join(root, 'crates/track-format/src'));
  checkDependencies(metadata, 'compiler-cli');
  checkDependencies(metadata, 'track-compiler-core');
  sources(join(root, 'crates/track-compiler-core/src'), false);
  // sim-core is part of the deterministic core too (story 0.7): same std and serde rules, plus Rapier.
  sources(join(root, 'crates/sim-core/src'), true, ['rapier3d']);
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) { check(); console.log('track-format layers and serde: OK'); }
