import { FatalScreen } from './components/FatalScreen.tsx';
import { Loading, Notice } from './components/StatusLayer.tsx';
import { useStore } from './hooks/useStore.ts';
import { Hud } from './hud/Hud.tsx';
import type { ExternalStore, FatalActions, UiSnapshot } from './ui-snapshot.ts';

/** Everything the player sees over the 3D view: HUD, loading, notices and the fatal state. */
export function UiRoot({ store, fatal }: { store: ExternalStore<UiSnapshot>; fatal: FatalActions }) {
  const snap = useStore(store);
  if (snap.error) return <FatalScreen error={snap.error} actions={fatal} />;
  const loading = !snap.ready || !snap.rendererReady;
  return (
    <>
      {loading ? <Loading /> : <Hud snap={snap} />}
      {snap.notice && <Notice notice={snap.notice} />}
    </>
  );
}
