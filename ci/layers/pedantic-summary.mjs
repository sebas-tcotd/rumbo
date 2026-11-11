#!/usr/bin/env node
// Reads `cargo clippy --message-format=json` from stdin and prints a Markdown summary of the
// warnings (pedantic included) for the CI step summary. Never fails: pedantic is a warning (story 0.7).
//
//   cargo clippy --workspace --all-targets --locked --message-format=json | node ci/layers/pedantic-summary.mjs

import { pathToFileURL } from 'node:url';

/** Counts warnings by lint code, deduplicating diagnostics that cargo reports for several targets. */
export function countWarnings(text) {
  const seen = new Set();
  const counts = new Map();
  for (const line of text.split('\n')) {
    if (!line.startsWith('{')) continue;
    const message = JSON.parse(line);
    const diagnostic = message.reason === 'compiler-message' ? message.message : null;
    if (diagnostic?.level !== 'warning' || !diagnostic.code) continue;
    const span = diagnostic.spans[0];
    const key = `${diagnostic.code.code}|${span?.file_name}|${span?.line_start}|${span?.column_start}`;
    if (seen.has(key)) continue;
    seen.add(key);
    counts.set(diagnostic.code.code, (counts.get(diagnostic.code.code) ?? 0) + 1);
  }
  return counts;
}

/** Markdown table, most frequent lint first. */
export function summary(counts) {
  const total = [...counts.values()].reduce((sum, n) => sum + n, 0);
  const rows = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  const lines = [`### Clippy warnings (pedantic, not blocking): ${total}`, '', '| Lint | Count |', '|---|---|'];
  for (const [code, n] of rows) lines.push(`| \`${code}\` | ${n} |`);
  return lines.join('\n') + '\n';
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  // Async read: a synchronous read of fd 0 stops early on Windows pipes.
  let text = '';
  process.stdin.setEncoding('utf8');
  for await (const chunk of process.stdin) text += chunk;
  process.stdout.write(summary(countWarnings(text)));
}
