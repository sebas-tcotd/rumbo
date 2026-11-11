/** Page visibility as an event source; the host pauses the race while the tab is hidden (P2). */
export interface VisibilitySource {
  readonly hidden: boolean;
  /** Calls `listener` on every change; returns the unsubscribe function. */
  subscribe(listener: (hidden: boolean) => void): () => void;
}

/** VisibilitySource over `document.visibilityState`. */
export function createVisibilitySource(doc: Document): VisibilitySource {
  return {
    get hidden() {
      return doc.hidden;
    },
    subscribe(listener) {
      const onChange = () => listener(doc.hidden);
      doc.addEventListener('visibilitychange', onChange);
      return () => doc.removeEventListener('visibilitychange', onChange);
    },
  };
}
