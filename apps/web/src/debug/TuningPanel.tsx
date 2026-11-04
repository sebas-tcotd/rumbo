import { useEffect, useMemo, useRef, useState } from 'react';
import type { ParamMeta, ProfileFile, ProfileParams } from '../sim/protocol';
import { download } from './download';

interface Props {
  schema: ParamMeta[];
  applied: ProfileFile;
  onApply: (p: ProfileFile) => void;
}

const DRAFT_KEY = 'race-lab:draft-profile';

function loadDraft(applied: ProfileFile, schema: ParamMeta[]): ProfileParams {
  try {
    const raw = localStorage.getItem(DRAFT_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as ProfileParams;
      if (schema.every((m) => typeof parsed[m.key] === 'number')) return parsed;
    }
  } catch {
    /* sin almacenamiento: se usa el perfil aplicado */
  }
  return { ...applied.params };
}

const decimals = (step: number) => Math.max(0, Math.ceil(-Math.log10(step)));

export function TuningPanel({ schema, applied, onApply }: Props) {
  const [draft, setDraft] = useState<ProfileParams>(() => loadDraft(applied, schema));
  const [importError, setImportError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    try {
      localStorage.setItem(DRAFT_KEY, JSON.stringify(draft));
    } catch {
      /* ignorar */
    }
  }, [draft]);

  const groups = useMemo(() => {
    const map = new Map<string, ParamMeta[]>();
    for (const m of schema) map.set(m.group, [...(map.get(m.group) ?? []), m]);
    return [...map.entries()];
  }, [schema]);

  const changed = schema.filter((m) => draft[m.key] !== applied.params[m.key]);
  const set = (key: string, v: number, meta: ParamMeta) => {
    if (!Number.isFinite(v)) return;
    setDraft((d) => ({ ...d, [key]: Math.min(meta.max, Math.max(meta.min, v)) }));
  };

  const importFile = async (f: File) => {
    try {
      const parsed = JSON.parse(await f.text()) as Partial<ProfileFile>;
      const params = parsed.params ?? {};
      const missing = schema.filter((m) => typeof params[m.key] !== 'number').map((m) => m.key);
      const unknown = Object.keys(params).filter((k) => !schema.some((m) => m.key === k));
      if (unknown.length) throw new Error(`Parámetros desconocidos: ${unknown.join(', ')}`);
      setDraft({ ...draft, ...params });
      setImportError(missing.length ? `Se conservaron los valores actuales para: ${missing.join(', ')}` : null);
    } catch (e) {
      setImportError(e instanceof Error ? e.message : 'No se pudo leer el archivo');
    }
  };

  return (
    <div className="tuning">
      <div className="tuning-actions">
        <button className="primary" disabled={!changed.length} onClick={() => onApply({ ...applied, params: { ...draft } })}>
          {changed.length ? `Aplicar ${changed.length} ${changed.length === 1 ? 'cambio' : 'cambios'}` : 'Sin cambios'}
        </button>
        <button disabled={!changed.length} onClick={() => setDraft({ ...applied.params })}>
          Descartar
        </button>
      </div>
      <p className="note">Aplicar empieza una sesión nueva: el auto vuelve a la grilla y el replay arranca de cero.</p>

      {groups.map(([group, params]) => (
        <fieldset key={group} className="param-group">
          <legend>{group}</legend>
          {params.map((m) => {
            const v = draft[m.key];
            const dirty = v !== applied.params[m.key];
            const isDefault = v === m.default;
            const isToggle = m.min === 0 && m.max === 1 && m.step === 1;
            if (isToggle) {
              return (
                <div key={m.key} className={dirty ? 'param dirty' : 'param'}>
                  <label className="toggle">
                    <input type="checkbox" checked={v >= 0.5} onChange={(e) => set(m.key, e.target.checked ? 1 : 0, m)} />
                    {m.label}
                  </label>
                  {m.help && <div className="param-help">{m.help}</div>}
                </div>
              );
            }
            return (
              <div key={m.key} className={dirty ? 'param dirty' : 'param'}>
                <label htmlFor={`p-${m.key}`} title={m.help || undefined}>
                  {m.label}
                </label>
                <div className="param-row">
                  <input
                    id={`p-${m.key}`}
                    type="range"
                    min={m.min}
                    max={m.max}
                    step={m.step}
                    value={v}
                    onChange={(e) => set(m.key, Number(e.target.value), m)}
                  />
                  <input
                    className="num"
                    type="number"
                    min={m.min}
                    max={m.max}
                    step={m.step}
                    value={Number(v.toFixed(decimals(m.step)))}
                    aria-label={`${m.label} en ${m.unit}`}
                    onChange={(e) => set(m.key, Number(e.target.value), m)}
                  />
                  <span className="unit">{m.unit}</span>
                </div>
                {(m.help || !isDefault) && (
                  <div className="param-help">
                    {m.help}
                    {!isDefault && (
                      <button className="link" onClick={() => set(m.key, m.default, m)}>
                        Volver a {m.default}
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </fieldset>
      ))}

      <div className="tuning-files">
        <button onClick={() => download(`${applied.id}.json`, JSON.stringify({ ...applied, params: draft }, null, 2))}>
          Exportar perfil
        </button>
        <button onClick={() => fileRef.current?.click()}>Importar perfil</button>
        <button
          onClick={() => setDraft(Object.fromEntries(schema.map((m) => [m.key, m.default])))}
        >
          Valores por defecto
        </button>
        <input
          ref={fileRef}
          type="file"
          accept="application/json"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) importFile(f);
            e.target.value = '';
          }}
        />
      </div>
      {importError && <p className="error">{importError}</p>}
    </div>
  );
}
