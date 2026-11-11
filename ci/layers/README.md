# rumbo-layers

Layer guardrails (ADR-005, P1, story 0.7). Every layer rule has a tool that enforces it, and every tool
has a test with intentional violations that proves it is still alive.

| File | What it does |
|---|---|
| `policy.mjs` | Single source: what each pnpm package and Cargo member may import or depend on |
| `biome.mjs` | Writes `biome.json` from the policy; `--check` fails if the file drifted. **Do not edit `biome.json` by hand** (it allows no comments, so this is the only place that says so) |
| `check.mjs` | Meta-check: every package/crate declared, `@rumbo/*` deps ⊆ policy, deps == tsconfig references, `[lints] workspace = true`, core `clippy.toml` == `core-clippy.toml`, Biome messages cite an ADR, no randomness or wall-clock crate in a core dependency graph |
| `core-clippy.toml` | Canonical `clippy.toml` of `track-format`, `sim-core` and `track-compiler-core` |
| `biome-guardrail.test.mjs` | Writes violating imports into the real packages and expects Biome to reject them |
| `clippy-guardrail.test.mjs` | Writes violating examples into the real crates and expects the blocking Clippy run to fail with each lint (slow: `test:clippy`) |
| `pedantic-summary.mjs` | Counts Clippy warnings for the CI step summary (pedantic never blocks) |

## Adding a package or crate

1. Add its entry to `policy.mjs`: `dir`, `exts`, `layout`, `rumbo` (allowed `@rumbo/*`), `forbid` (Biome
   groups with a message that says what to use instead and cites the ADR), and guardrail cases
   (`allowed`, at least one `rejected`). Tooling packages declare `tooling: '<why>'` instead.
2. `node ci/layers/biome.mjs`, then `node ci/layers/check.mjs`.
3. Add its project reference in the consumers' `tsconfig.json` (and `workspace:*` dependency).
4. A crate: add it to `RUST` with its `internal` crates, add `[lints]\nworkspace = true` to its
   `Cargo.toml`, and if it is part of the deterministic core, set `core: true` and copy `core-clippy.toml`
   next to its `Cargo.toml`.

## Known limits

- Biome compares import specifiers, not resolved paths. Relative escapes are written per folder depth;
  files deeper than `NESTED_DEPTHS` share one rule. `./.././x` is rejected everywhere, but `../a/../../x`,
  `require()` and `import.meta.glob()` are invisible to Biome. In the packages, `tsc -b` rejects them
  (TS2307 under `rootDir`, TS2591 without Node types, TS2339 without Vite types), checked by hand on
  2026-11-11. pnpm isolation also makes an undeclared `@rumbo/*` import unresolvable.
- Clippy cannot tell iterating a `HashMap` from looking it up, so the core bans the type.
- A `clippy.toml` path that does not resolve is only a configuration warning: the Clippy guardrail covers
  each family (collections, time, fs, floats) so a typo cannot leave a rule dead unnoticed.
- `rand` is blocked through the dependency graph (`check.mjs`), not through `clippy.toml`: paths of crates
  that are not dependencies do not resolve.

## Running

```bash
node ci/layers/check.mjs
pnpm exec biome lint
pnpm --filter rumbo-layers test
cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::pedantic
pnpm --filter rumbo-layers run test:clippy
```
