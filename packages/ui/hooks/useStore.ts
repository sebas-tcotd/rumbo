import { useSyncExternalStore } from 'react';
import type { ExternalStore } from '../ui-snapshot.ts';

/** Subscribes a component to an injected external store. */
export function useStore<T>(store: ExternalStore<T>): T {
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}
