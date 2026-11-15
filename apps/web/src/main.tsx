// Composition root (ADR-001): the only module that knows every adapter. Each package receives its
// dependencies here through its factory; nothing is a singleton or looked up globally.

import defaultProfile from '@assets/profiles/hatch-arcade.json';
import { createCounterRing, createLogger, formatDiagnostic, PROTOCOL_VERSION, type LogChannel, type ProfileFile,
  type RequiredCapability } from '@rumbo/contracts';
import { copyText, createLocalSettingsStore, createWebPlatform, missingRequired, probeCapabilities, safeLocalStorage } from '@rumbo/platform-web';
import { createThreeRenderer } from '@rumbo/render-three';
import { createWorkerSim } from '@rumbo/sim-worker';
import { mountUi, type FatalActions, type FatalView, type UiSnapshot } from '@rumbo/ui';
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

// Story 0.12 can point this link to the project's own domain.
const supportedBrowsersUrl = 'https://github.com/sebas-tcotd/rumbo#navegadores-soportados';
const capabilityChannels: Record<RequiredCapability, LogChannel> = {
  webgl2: 'render', wasm: 'sim', compressionStream: 'net', moduleWorker: 'sim', indexedDb: 'storage',
};

function startupDiagnostics() {
  const logger = createLogger({ capacity: 256, consoleLevel: import.meta.env.DEV ? 'debug' : 'warn', sink: console,
    now: () => performance.now() });
  const capabilities = probeCapabilities(window);
  const counters = createCounterRing({ capacity: 60 });
  return { logger, capabilities, counters };
}

type Diagnostics = ReturnType<typeof startupDiagnostics>;

function fatalActions(diagnostics: Diagnostics, read: () => { error: FatalView | null; backend: string; physicsVersion?: string }): FatalActions {
  return {
    retry: () => window.location.reload(),
    copy: (text) => copyText(window, text),
    supportedBrowsersUrl,
    diagnostic: () => {
      const { error, backend, physicsVersion } = read();
      const log = diagnostics.logger.records();
      const code = error?.code ?? 'SIM_FAILED';
      const record = log.filter((entry) => entry.code === code).at(-1);
      return formatDiagnostic({
        build: import.meta.env.RUMBO_BUILD, copiedAt: new Date().toISOString(), code,
        channel: record?.channel ?? 'game', detail: record?.fields.detail?.toString(), physicsVersion,
        protocolVersion: PROTOCOL_VERSION, backend, browser: window.navigator.userAgent,
        capabilities: diagnostics.capabilities, log, counters: diagnostics.counters.samples(),
      });
    },
  };
}

function mountStartupFatal(root: HTMLElement, uiRoot: HTMLElement, error: FatalView, diagnostics: Diagnostics) {
  const snapshot: UiSnapshot = {
    error, ready: null, index: null, state: null, appliedProfile: null, ghost: null,
    autopilot: false, paused: false, rendererReady: false, notice: null,
  };
  const ui = mountUi(uiRoot, {
    store: { getSnapshot: () => snapshot, subscribe: () => () => {} },
    settings: createLocalSettingsStore(safeLocalStorage()),
    fatal: fatalActions(diagnostics, () => ({ error, backend: 'none' })),
  });
  return () => { ui.unmount(); root.replaceChildren(); };
}

/** Wires the lab and returns its teardown. */
function start(root: HTMLElement): () => void {
  const diagnostics = startupDiagnostics();
  const { logger, capabilities } = diagnostics;
  const dom = buildLayout(root);
  const missing = missingRequired(capabilities);
  if (missing.length) {
    logger.log('error', 'game', 'BROWSER_UNSUPPORTED', { missing });
    for (const name of missing) logger.log('warn', capabilityChannels[name], 'REQUIRED_CAPABILITY_MISSING', { name });
    dom.lab.classList.add('panel-hidden');
    return mountStartupFatal(root, dom.uiRoot, { code: 'BROWSER_UNSUPPORTED', missing }, diagnostics);
  }
  return startLab(root, dom, diagnostics);
}

function startLab(root: HTMLElement, dom: ReturnType<typeof buildLayout>, diagnostics: Diagnostics): () => void {
  const { logger, counters } = diagnostics;
  const track = selectedTrack();
  const profile = defaultProfile as ProfileFile;
  let renderer: ReturnType<typeof createThreeRenderer>;
  try {
    renderer = createThreeRenderer(dom.renderHost);
  } catch (detail) {
    logger.log('error', 'render', 'RENDER_INIT_FAILED', { detail });
    return mountStartupFatal(root, dom.uiRoot, { code: 'RENDER_INIT_FAILED' }, diagnostics);
  }
  let session: LabSession;
  try {
    session = new LabSession((receive) => createWorkerSim(receive, logger), logger);
  } catch (detail) {
    renderer.dispose();
    logger.log('error', 'sim', 'SIM_FAILED', { detail });
    return mountStartupFatal(root, dom.uiRoot, { code: 'SIM_FAILED' }, diagnostics);
  }
  const controller = createLabController({ session, renderer, switchTrack });
  const platform = createWebPlatform({ window, storage: safeLocalStorage(), onKeyAction: controller.handleKey,
    gamepadFailure: diagnostics.capabilities.gamepadFailure, logger });
  let loop: FrameLoop | null = null;
  let devtools: { unmount(): void } | null = null;
  let disposed = false;
  const timer = window.setInterval(() => {
    counters.push({
      t: performance.now(), simMs: session.latest?.stepMs ?? 0, simHz: session.latest?.simHz ?? 0,
      frameMs: loop?.takeFrameMs() ?? 0,
    });
  }, 1000);

  session.onReady = (ready, index) => {
    loop?.resetInput();
    renderer.build(ready, index);
  };
  session.onGhost = (frames) => renderer.setGhost(frames);
  // A seek runs the replay's profile: the car's rest height follows whatever profile is running.
  session.onProfile = (p) => renderer.setSuspensionRest(p.params.suspension_rest_m);
  // A fatal error stops drawing and sampling input behind the error screen.
  const stopVisibility = platform.visibility.subscribe((hidden) => session.setHidden(hidden));
  const stopActivity = () => {
    loop?.stop();
    window.clearInterval(timer);
    stopVisibility();
    platform.dispose();
    renderer.dispose();
  };
  session.onFatal = stopActivity;
  session.setHidden(platform.visibility.hidden);
  // Without devtools there is no panel: the viewport takes the whole width.
  const syncLayout = () =>
    dom.lab.classList.toggle('panel-hidden', !(import.meta.env.RUMBO_DEVTOOLS && session.getSnapshot().panelOpen));
  const stopLayout = session.subscribe(syncLayout);

  session.init(track.json, profile);
  renderer.setSuspensionRest(profile.params.suspension_rest_m);
  renderer.init().then(
    () => {
      // The simulation may have failed while the renderer was starting: no loop behind the error.
      if (disposed || session.getSnapshot().error !== null) return;
      if (renderer.backend === 'WebGL 2') logger.log('warn', 'render', 'WEBGPU_UNAVAILABLE');
      session.patch({ rendererReady: true, backend: renderer.backend });
      loop = startFrameLoop({ session, renderer, input: platform.input, isHidden: () => platform.visibility.hidden });
    },
    (error: unknown) => {
      if (!disposed) session.fail('RENDER_INIT_FAILED', error);
    },
  );
  const ui = mountUi(dom.uiRoot, { store: session, settings: platform.settings,
    fatal: fatalActions(diagnostics, () => ({
      error: session.getSnapshot().error,
      physicsVersion: session.getSnapshot().ready?.physicsVersion,
      backend: renderer.backend === 'iniciando' ? 'none' : renderer.backend,
    })),
  });

  if (import.meta.env.RUMBO_DEVTOOLS) {
    import('@rumbo/devtools').then(
      ({ mountDevtools }) => {
        if (disposed) return;
        devtools = mountDevtools(
          { panel: dom.panel, toolbar: dom.toolbar },
          { store: session, actions: controller, events: controller, track, tracks: TRACKS, debug: renderer },
        );
      },
      (error: unknown) => {
        // The lab keeps driving without its panel; say why instead of leaving an empty column.
        logger.log('error', 'game', 'DEVTOOLS_LOAD_FAILED', { detail: error });
        if (!disposed) dom.panel.textContent = 'No se pudieron cargar las herramientas del laboratorio.';
      },
    );
  }

  return () => {
    disposed = true;
    stopActivity();
    devtools?.unmount();
    ui.unmount();
    stopLayout();
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
