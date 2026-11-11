import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createLocalSettingsStore } from './local-settings-store.ts';
import { migrateSettings, SETTINGS_KEY, serializeSettings } from './settings-schema.ts';

const fixture = (name) => readFileSync(new URL('./fixtures/' + name, import.meta.url), 'utf8').trim();
const DEFAULTS = { locale: 'es' };

test('valid v1 data loads as is', () => {
  assert.deepEqual(migrateSettings(fixture('v1-valid.json')), { settings: { locale: 'es' }, persist: true });
});

test('corrupt data falls back to defaults without throwing', () => {
  for (const name of ['truncated.txt', 'no-version.json', 'fractional-version.json']) {
    assert.deepEqual(migrateSettings(fixture(name)), { settings: DEFAULTS, persist: true }, name);
  }
  for (const raw of [null, '', 'null', '[]', '"es"', '{"version":0}', '{"version":-1}', '{"version":"1"}']) {
    assert.deepEqual(migrateSettings(raw).settings, DEFAULTS, String(raw));
  }
});

test('an invalid field takes its default', () => {
  assert.deepEqual(migrateSettings(fixture('v1-bad-locale.json')).settings, DEFAULTS);
  assert.deepEqual(migrateSettings(fixture('v1-wrong-type.json')).settings, DEFAULTS);
});

test('data from a newer build is read as defaults and must not be persisted', () => {
  assert.deepEqual(migrateSettings(fixture('v9-future.json')), { settings: DEFAULTS, persist: false });
});

test('migrations are chained in order from the stored version', () => {
  const calls = [];
  const chain = [
    (v1) => {
      calls.push('1-2');
      return { ...v1, volume: 0.5 };
    },
    (v2) => {
      calls.push('2-3');
      return { ...v2, locale: v2.lang ?? v2.locale };
    },
  ];
  const result = migrateSettings('{"version":1,"lang":"es","locale":"xx"}', chain, 3);
  assert.deepEqual(calls, ['1-2', '2-3']);
  assert.deepEqual(result, { settings: { locale: 'es' }, persist: true });
  calls.length = 0;
  migrateSettings('{"version":2,"locale":"es"}', chain, 3);
  assert.deepEqual(calls, ['2-3'], 'starts at the stored version');
  assert.deepEqual(migrateSettings('{"version":1}', [], 2).settings, DEFAULTS, 'a missing step yields defaults');
});

test('serialised settings round-trip with their version', () => {
  assert.equal(serializeSettings({ locale: 'es' }), '{"version":1,"locale":"es"}');
  assert.deepEqual(migrateSettings(serializeSettings({ locale: 'es' })).settings, { locale: 'es' });
});

function memoryStorage(initial) {
  const map = new Map(initial ? [[SETTINGS_KEY, initial]] : []);
  return { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, v), map };
}

test('the store saves under rumbo.settings.v1 and reloads', () => {
  const storage = memoryStorage();
  const store = createLocalSettingsStore(storage);
  assert.deepEqual(store.load(), DEFAULTS);
  assert.deepEqual(store.save({ locale: 'es' }), { ok: true });
  assert.equal(storage.map.get(SETTINGS_KEY), '{"version":1,"locale":"es"}');
  assert.deepEqual(createLocalSettingsStore(storage).load(), { locale: 'es' });
});

test('an inaccessible storage leaves defaults and never throws', () => {
  const throwing = {
    getItem() {
      throw new Error('SecurityError');
    },
    setItem() {
      throw new Error('QuotaExceededError');
    },
  };
  const store = createLocalSettingsStore(throwing);
  assert.deepEqual(store.load(), DEFAULTS);
  assert.deepEqual(store.save({ locale: 'es' }), { ok: false, code: 'STORAGE_UNAVAILABLE' });
  const none = createLocalSettingsStore(null);
  assert.deepEqual(none.load(), DEFAULTS);
  assert.equal(none.save(DEFAULTS).ok, false);
});

test('data written by a newer build is not overwritten', () => {
  const storage = memoryStorage(fixture('v9-future.json'));
  const store = createLocalSettingsStore(storage);
  assert.deepEqual(store.load(), DEFAULTS);
  assert.equal(store.save({ locale: 'es' }).ok, false);
  assert.equal(storage.map.get(SETTINGS_KEY), fixture('v9-future.json'));
});
