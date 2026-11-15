import { useTranslation } from 'react-i18next';
import type { UiNotice } from '../ui-snapshot.ts';

/** Loading message until both the simulation and the renderer are ready. */
export function Loading() {
  const { t } = useTranslation();
  return <div className="loading">{t('lab.loading')}</div>;
}

/** Non-fatal notice (rejected command); the session keeps running. */
export function Notice({ notice }: { notice: UiNotice }) {
  const { t } = useTranslation();
  return (
    <div className="loading" role="status">
      {t(`errors.${notice.code}`, { operation: notice.operation ?? '' })}
    </div>
  );
}
