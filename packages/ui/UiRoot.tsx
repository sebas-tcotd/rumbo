import { Fatal, Loading, Notice } from './components/StatusLayer.tsx';
import { useStore } from './hooks/useStore.ts';
import { Hud } from './hud/Hud.tsx';
import type { ExternalStore, UiSnapshot } from './ui-snapshot.ts';

/** Everything the player sees over the 3D view: HUD, loading, notices and the fatal state. */
export function UiRoot({ store }: { store: ExternalStore<UiSnapshot> }) {
  const snap = useStore(store);
  const loading = !snap.ready || !snap.rendererReady;
  return (
    <>
      {!loading && <Hud snap={snap} />}
      {loading && !snap.error && <Loading />}
      {snap.notice && !snap.error && <Notice notice={snap.notice} />}
      {snap.error && <Fatal message={snap.error} />}
    </>
  );
}
