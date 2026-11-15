import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formatDiagnostic } from './dist/index.js';

test('a diagnostic includes build, error, capabilities, counters and logs without machine paths or IPs', () => {
  const result = formatDiagnostic({
    build: 'abcdef', copiedAt: '2026-11-13T12:00:00.000Z', code: 'SIM_FAILED', channel: 'sim',
    detail: 'Failed at file:///C:/Users/Test/car.js from /home/test/car.js and 192.168.1.1',
    physicsVersion: '0.5.0', protocolVersion: 2, backend: 'WebGL 2',
    browser: 'Mozilla/5.0 DeviceOwner Chrome/131.0.0.0 Safari/537.36',
    capabilities: { required: { wasm: true }, optional: { gamepad: false } },
    log: [
      { t: 1, level: 'warn', channel: 'input', code: 'GAMEPAD_UNAVAILABLE', fields: { reason: 'blocked' } },
      { t: 2, level: 'error', channel: 'sim', code: 'SIM_FAILED', fields: {
        asset: 'https://example.com/path?token=private',
        path: '"C:\\Users\\Sample Name\\asset"',
        detail: String.raw`open \\example.invalid\private\Alice Doe\replay.json, /private.csv or C:\Users\Bob Doe\replay.json at [::]`,
      } },
      { t: 3, level: 'error', channel: 'sim', code: 'SIM_FAILED', fields: { detail: 'unreachable' } },
    ],
    counters: [{ t: 1000, simMs: 1, simHz: 120, frameMs: 16 }],
  });
  assert.match(result, /abcdef/);
  assert.match(result, /SIM_FAILED \(fatal, channel sim\)/);
  assert.match(result, /physics: 0\.5\.0.*protocol: 2.*backend: WebGL 2/);
  assert.match(result, /wasm=yes gamepad=no/);
  assert.match(result, /1000.*1.*120.*16/);
  assert.match(result, /WARN input GAMEPAD_UNAVAILABLE reason=blocked[\s\S]+ERROR sim SIM_FAILED/);
  assert.match(result, /browser: Chrome\/131\.0\.0\.0/);
  assert.match(result, /detail=<redacted>[\s\S]+detail=unreachable/);
  assert.doesNotMatch(result, /file:\/\/\/|C:[/\\]|\/home\/|192\.168\.1\.1|token=private|Sample Name|Alice Doe|Bob Doe|private\.csv|\[::\]|DeviceOwner|example\.invalid/);
});
