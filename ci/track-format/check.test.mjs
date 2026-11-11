import { test } from 'node:test';
import assert from 'node:assert/strict';
import { checkDependencies, checkSource } from './check.mjs';
function graph(kind = null) { return { packages: [{ id: 'kernel', name: 'track-format' }, { id: 'bad', name: 'sim-core' }], resolve: { nodes: [{ id: 'kernel', deps: kind === null ? [] : [{ pkg: 'bad', dep_kinds: [{ kind }] }] }, { id: 'bad', deps: [] }] } }; }
test('valid dependency graph passes', () => checkDependencies(graph()));
for (const kind of [null, 'build']) test(`rejects ${kind ?? 'normal'} runtime dependency`, () => { const m = graph('build'); m.resolve.nodes[0].deps[0].dep_kinds[0].kind = kind; assert.throws(() => checkDependencies(m), /forbidden/); });
test('dev edge cannot mask a normal edge', () => {const m=graph('dev');m.resolve.nodes[0].deps[0].dep_kinds.push({kind:null});assert.throws(()=>checkDependencies(m),/forbidden/);});
test('missing kernel fails closed',()=>assert.throws(()=>checkDependencies({packages:[],resolve:{nodes:[]}}),/missing/));
test('allowed serde passes',()=>checkSource('#[serde(default, rename_all="camelCase")]'));
for(const attr of ['flatten','skip_serializing_if = "Option::is_none"']) test(`rejects ${attr}`,()=>assert.throws(()=>checkSource(`#[serde(\n default, ${attr})]`),/forbidden/));
test('rejects runtime IO',()=>assert.throws(()=>checkSource('std::fs::read(path)'),/forbidden/));

for (const attribute of [
  '#[cfg_attr(all(), serde(flatten))]',
  '#[cfg_attr(all(), serde(skip_serializing_if = "Option::is_none"))]',
  '#[cfg_attr(all(), cfg_attr(all(), serde(flatten)))]',
  '#[serde(rename = "field]", flatten)]',
  '#[serde(rename = r#"field)]"#, skip_serializing_if = "Option::is_none")]',
  '#[serde(/* ] */ r#flatten)]',
]) {
  test(`rejects nested or token-obscured serde: ${attribute}`, () => {
    assert.throws(() => checkSource(attribute), /forbidden serde/);
  });
}

for (const module of ['fs', 'net', 'time']) {
  for (const source of [
    `use std::{${module}};`,
    `use std::{fmt, ${module} as runtime};`,
    `use std::{${module}::{self as runtime}};`,
    `use {std::{${module}}};`,
    `use std /* comment */ :: r#${module} as runtime;`,
  ]) {
    test(`rejects runtime import: ${source}`, () => assert.throws(() => checkSource(source), /forbidden kernel runtime/));
  }
}
for (const source of [
  'use std as platform; platform::fs::read("x");',
  'extern crate std as platform;',
  'use std::*;',
  'use std::{self as platform, fmt};',
]) {
  test(`rejects opaque std imports: ${source}`, () => assert.throws(() => checkSource(source), /forbidden kernel runtime/));
}

for (const source of [
  'use std::{fmt::{self, Display}, error::Error};',
  'use std::fmt as formatting;',
  '#[cfg_attr(all(), serde(default, rename = "flatten"))]',
  '#[serde(rename = "skip_serializing_if")]',
  '// #[serde(flatten)]\n/* std::fs /* nested */ ::read() */',
  'let text = "std::fs::read #[serde(flatten)]";',
  'let text = r###"std::net #[serde(skip_serializing_if = "]")]"###;',
  'let text = br#"std::time #[serde(flatten)]"#;',
  "fn text<'a>(s: &'a str) { let quote = '\"'; let slash = b'/'; }",
]) {
  test(`allows safe Rust syntax: ${source}`, () => checkSource(source));
}

test('compiler permits legacy JSON attributes and rejects runtime imports', () => {
  checkSource('#[serde(skip_serializing_if = "Option::is_none")]', 'compiler', false);
  for (const source of ['use std::{fs};', 'use std::{fmt, net as sockets};', 'use std::time;', 'use std as platform;']) {
    assert.throws(() => checkSource(source, 'compiler', false), /forbidden/);
  }
});

for (const source of [
  'std::io::stdin().read_line(&mut String::new())',
  'std::io::stdout()',
  'std::io::stderr()',
  'std::process::Command::new("cat").arg("data.json").output()',
  'use std::io as streams; streams::stdin()',
  'use std::process::Command as Runner;',
  'use std::{fmt, io::{stdin as input}};',
  'use {std::{process::{Command}}};',
  'use std /* comment */ :: r#io :: stdin;',
  'use std::{io};',
  'use std::io::*;',
  'use std::process::*;',
]) {
  test(`compiler rejects external IO: ${source}`, () => {
    assert.throws(() => checkSource(source, 'compiler', false), /forbidden kernel runtime/);
  });
}

test('compiler IO policy ignores literals and permits pure code', () => {
  checkSource('// std::io::stdin()\nlet description = "std::process::Command";', 'compiler', false);
  checkSource('use std::{fmt::Display, error::Error};', 'compiler', false);
  // Keep the existing kernel policy independent from the compiler policy.
  checkSource('use std::io::Cursor;');
});

test('compiler rejects normal and build dependencies on runtime crates', () => {
  for (const kind of ['build', null]) {
    for (const name of ['sim-core', 'tiff', 'compiler-cli']) {
      const metadata = graph('build');
      metadata.packages[0].name = 'track-compiler-core';
      metadata.packages[1].name = name;
      metadata.resolve.nodes[0].deps[0].dep_kinds[0].kind = kind;
      assert.throws(() => checkDependencies(metadata, 'track-compiler-core'), /forbidden/);
    }
  }
});

test('CLI isolates simulation in dev dependencies', () => {
  const metadata = graph('dev');
  metadata.packages[0].name = 'compiler-cli';
  checkDependencies(metadata, 'compiler-cli');
  for (const kind of [null, 'build']) {
    metadata.resolve.nodes[0].deps[0].dep_kinds = [{ kind }];
    assert.throws(() => checkDependencies(metadata, 'compiler-cli'), /forbidden/);
  }
});

test('sim-core mode allows Rapier and still rejects runtime I/O, time and randomness', () => {
  checkSource('use rapier3d::prelude::*;', 'sim-core', true, ['rapier3d']);
  for (const source of ['use std::time::Instant;', 'std::fs::read(path)', 'use std::net::TcpStream;', 'rand::random::<u32>()', 'wasm_bindgen::JsValue']) {
    assert.throws(() => checkSource(source, 'sim-core', true, ['rapier3d']), /forbidden kernel runtime/, source);
  }
  assert.throws(() => checkSource('use rapier3d::prelude::*;'), /forbidden kernel runtime/);
});
