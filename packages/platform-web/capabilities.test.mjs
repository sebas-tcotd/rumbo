import assert from 'node:assert/strict';
import { test } from 'node:test';
import { missingRequired, probeCapabilities } from './capabilities.ts';
import { copyText } from './clipboard.ts';

function browser() {
  return {
    document: { createElement: () => ({ getContext: () => ({ getExtension: () => null }) }) },
    WebAssembly,
    CompressionStream,
    DecompressionStream,
    Worker: class {
      constructor(_url, options) {
        assert.equal(options.type, 'module');
        throw new TypeError('Invalid URL');
      }
    },
    indexedDB: { open() {} },
    navigator: {
      getGamepads: () => [],
      get userAgent() { assert.fail('Capability probing must not inspect the UA'); },
    },
  };
}

test('reports each missing requirement, including APIs that throw', () => {
  const win = browser();
  assert.deepEqual(missingRequired(probeCapabilities(win)), []);
  win.document.createElement = () => ({ getContext: () => null });
  win.WebAssembly = { validate: () => true, Module: class { constructor() { throw new Error('CSP'); } } };
  win.DecompressionStream = class { constructor() { throw new TypeError(); } };
  win.Worker = undefined;
  win.indexedDB = null;
  win.navigator.getGamepads = () => { throw new Error('permissions policy'); };
  const report = probeCapabilities(win);
  assert.deepEqual(missingRequired(report), ['webgl2', 'wasm', 'compressionStream', 'moduleWorker', 'indexedDb']);
  assert.equal(report.optional.gamepad, false);
  assert.equal(report.gamepadFailure, 'blocked');
});

test('clipboard success, absence, and denial are reported without throwing', async () => {
  const win = browser();
  assert.equal(await copyText(win, 'sample'), false);
  win.navigator.clipboard = { writeText: async (text) => assert.equal(text, 'sample') };
  assert.equal(await copyText(win, 'sample'), true);
  Object.defineProperty(win.navigator, 'clipboard', { get() { throw new Error('blocked'); } });
  assert.equal(await copyText(win, 'sample'), false);
});
