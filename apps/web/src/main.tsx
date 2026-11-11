// Composition root (ADR-001): the only module that knows every adapter. Each package receives its
// dependencies here through its factory; nothing is a singleton or looked up globally.

import defaultProfile from '@assets/profiles/hatch-arcade.json';
import type { ProfileFile } from '@rumbo/contracts';
import { createWebPlatform, safeLocalStorage } from '@rumbo/platform-web';
import { createThreeRenderer } from '@rumbo/render-three';
import { createWorkerSim } from '@rumbo/sim-worker';
import { mountUi } from '@rumbo/ui';
import { type FrameLoop, startFrameLoop } from './frame-loop.ts';
import { createLabController } from './lab-controller.ts';
import { LabSession } from './lab-session.ts';
import { selectedTrack, switchTrack, TRACKS } from './tracks.ts';
import './styles.css';

/**
 * Page skeleton: 3D viewport (absolutely positioned render host, so the canvas size never feeds back
 * into the grid layout) with the UI overlay and the devtools slots. Lab builds reserve the panel column
 * from the start, as the PoC did, so the viewport does not change width when the devtools chunk loads.
 */
function buildLayout(root: HTMLElement) {
  const el = (className: string, parent: HTMLElement) => {
    const node = document.createElement('div');
    node.className = className;
    parent.appendChild(node);
    return node;
  };
  root.replaceChildren();
  const lab = el(import.meta.env.RUMBO_DEVTOOLS ? 'lab' : 'lab panel-hidden', root);
  const viewport = el('viewport', lab);
  const renderHost = el('render-host', viewport);
  const uiRoot = el('ui-root', viewport);
  const toolbar = el('devtools-toolbar', viewport);
  const panel = el('devtools-panel-slot', lab);
  return { lab, renderHost, uiRoot, toolbar, panel };
}

/** Wires the lab and returns its teardown. */
function start(root: HTMLElement): () => void {
  const track = selectedTrack();
  const profile = defaultProfile as ProfileFile;
  const dom = buildLayout(root);
  const renderer = createThreeRenderer(dom.renderHost);
  const session = new LabSession((receive) => createWorkerSim(receive));
  const controller = createLabController({ session, renderer, switchTrack });
  const platform = createWebPlatform({ window, storage: safeLocalStorage(), onKeyAction: controller.handleKey });
  let loop: FrameLoop | null = null;
  let devtools: { unmount(): void } | null = null;
  let disposed = false;

  session.onReady = (ready, index) => {
    loop?.resetInput();
    renderer.build(ready, index);
  };
  session.onGhost = (frames) => renderer.setGhost(frames);
  const stopVisibility = platform.visibility.subscribe((hidden) => session.setHidden(hidden));
  session.setHidden(platform.visibility.hidden);
  // Without devtools there is no panel: the viewport takes the whole width.
  const syncLayout = () =>
    dom.lab.classList.toggle('panel-hidden', !(import.meta.env.RUMBO_DEVTOOLS && session.getSnapshot().panelOpen));
  const stopLayout = session.subscribe(syncLayout);

  session.init(track.json, profile);
  renderer.setSuspensionRest(profile.params.suspension_rest_m);
  renderer.init().then(
    () => {
      if (disposed) return;
      session.patch({ rendererReady: true, backend: renderer.backend });
      loop = startFrameLoop({ session, renderer, input: platform.input, isHidden: () => platform.visibility.hidden });
    },
    (error: unknown) => {
      if (!disposed) session.fail(error instanceof Error ? error.message : String(error));
    },
  );
  const ui = mountUi(dom.uiRoot, { store: session, settings: platform.settings });

  if (import.meta.env.RUMBO_DEVTOOLS) {
    import('@rumbo/devtools').then(({ mountDevtools }) => {
      if (disposed) return;
      devtools = mountDevtools(
        { panel: dom.panel, toolbar: dom.toolbar },
        { store: session, actions: controller, events: controller, track, tracks: TRACKS },
      );
    });
  }

  return () => {
    disposed = true;
    loop?.stop();
    devtools?.unmount();
    ui.unmount();
    stopLayout();
    stopVisibility();
    platform.dispose();
    renderer.dispose();
    session.dispose();
    root.replaceChildren();
  };
}

const root = document.getElementById('root');
if (!root) throw new Error('ROOT_ELEMENT_MISSING');
const dispose = start(root);
// HMR: replacing this module tears the old lab down first, so no worker or loop is left running twice.
if (import.meta.hot) {
  import.meta.hot.dispose(dispose);
  import.meta.hot.accept();
}
