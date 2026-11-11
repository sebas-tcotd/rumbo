import type { RenderDebugPort } from '@rumbo/contracts';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { DevPanel } from './DevPanel.tsx';
import type { DevActions, DevEvents, DevStore, DevTrack } from './dev-snapshot.ts';
import { download } from './panels/download.ts';
import './devtools.css';

/**
 * Marker compiled into every devtools chunk. The bundle check (ci/bundle/check-devtools.mjs) fails if it
 * appears in the publication build `dist/` and if it is missing from the lab build `dist-lab/`.
 */
export const DEVTOOLS_MARKER = 'rumbo-devtools-bundle-marker';

/** Where devtools renders: the grid slot for the side panel and an element over the 3D view. */
export interface DevtoolsTargets {
  panel: HTMLElement;
  toolbar: HTMLElement;
}

/** What `mountDevtools` needs, injected by the composition root. */
export interface DevtoolsDeps {
  store: DevStore;
  actions: DevActions;
  events: DevEvents;
  track: DevTrack;
  tracks: DevTrack[];
  /** Renderer debug controls (overlays); devtools never imports the renderer. */
  debug: RenderDebugPort;
}

/** Handle returned by {@link mountDevtools}. */
export interface MountedDevtools {
  unmount(): void;
}

/** Mounts the lab panel in its own React root (separate from the player UI on purpose). */
export function mountDevtools(targets: DevtoolsTargets, deps: DevtoolsDeps): MountedDevtools {
  targets.panel.dataset.devtools = DEVTOOLS_MARKER;
  const stopReplays = deps.events.onReplay((json, ticks) => download(`replay-${ticks}-ticks.json`, json));
  const root = createRoot(targets.panel);
  root.render(
    <StrictMode>
      <DevPanel
        store={deps.store}
        actions={deps.actions}
        track={deps.track}
        tracks={deps.tracks}
        debug={deps.debug}
        toolbar={targets.toolbar}
      />
    </StrictMode>,
  );
  return {
    unmount() {
      stopReplays();
      root.unmount();
      delete targets.panel.dataset.devtools;
    },
  };
}
