import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { FatalActions, FatalView } from '../ui-snapshot.ts';

function ManualCopy({ text, label }: { text: string; label: string }) {
  const area = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { area.current?.focus(); area.current?.select(); }, []);
  return <textarea ref={area} aria-label={label} readOnly value={text} />;
}

/** Shows a localized fatal error and recovery actions. */
export function FatalScreen({ error, actions }: { error: FatalView; actions: FatalActions }) {
  const { t } = useTranslation();
  const title = useId();
  const detail = useId();
  const retry = useRef<HTMLButtonElement>(null);
  const copyRequest = useRef(0);
  const [copy, setCopy] = useState<{ status: 'copied' } | { status: 'failed'; text: string; request: number } | null>(null);
  useEffect(() => { retry.current?.focus(); }, []);

  async function copyDiagnostic() {
    const request = ++copyRequest.current;
    const text = actions.diagnostic();
    const copied = await actions.copy(text);
    if (request !== copyRequest.current) return;
    setCopy(copied ? { status: 'copied' } : { status: 'failed', text, request });
  }

  return (
    <section className="fatal" role="alertdialog" aria-labelledby={title} aria-describedby={detail}>
      <h1 id={title}>{t('fatal.title')}</h1>
      <p id={detail}>{t(`errors.${error.code}`)}</p>
      {error.code === 'BROWSER_UNSUPPORTED' && (
        <>
          <ul>{error.missing?.map((name) => <li key={name}>{t(`capability.${name}`)}</li>)}</ul>
          <p>{t('fatal.minimumVersions')}</p>
          <a href={actions.supportedBrowsersUrl} rel="noopener">{t('fatal.supportedBrowsersLink')}</a>
        </>
      )}
      <div className="fatal-actions">
        <button type="button" ref={retry} onClick={actions.retry}>{t('fatal.retry')}</button>
        <button type="button" onClick={copyDiagnostic}>{t('fatal.copyDiagnostic')}</button>
      </div>
      <p role="status">{copy?.status === 'copied' ? t('fatal.copied') : copy?.status === 'failed' ? t('fatal.copyFailed') : ''}</p>
      {copy?.status === 'failed' && <ManualCopy key={copy.request} text={copy.text} label={t('fatal.manualCopy')} />}
    </section>
  );
}
