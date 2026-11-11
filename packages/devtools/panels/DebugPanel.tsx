import type { OverlayKind, RenderDebugPort } from '@rumbo/contracts';

const OVERLAYS: [OverlayKind, string, string][] = [
  ['axis', 'Eje de la pista', 'Línea central que usa la simulación para medir el avance (s).'],
  ['checkpoints', 'Checkpoints', 'Líneas transversales en cada checkpoint del cronómetro.'],
  ['collision', 'Cajas de colisión', 'Caja del chasis sobre el auto y malla de los muros.'],
];

/** Props of {@link DebugPanel}. */
export interface DebugPanelProps {
  debug: RenderDebugPort;
  overlays: Record<OverlayKind, boolean>;
  onOverlays: (next: Record<OverlayKind, boolean>) => void;
}

/** Render debug overlays (AR28). The renderer is reached only through the injected RenderDebugPort. */
export function DebugPanel({ debug, overlays, onOverlays }: DebugPanelProps) {
  const toggle = (kind: OverlayKind, on: boolean) => {
    debug.setOverlay(kind, on);
    onOverlays({ ...overlays, [kind]: on });
  };
  return (
    <div className="session">
      <section>
        <h3>Overlays</h3>
        {OVERLAYS.map(([kind, label, help]) => (
          <div key={kind} className="param">
            <label className="toggle">
              <input type="checkbox" checked={overlays[kind]} onChange={(e) => toggle(kind, e.target.checked)} />
              {label}
            </label>
            <div className="param-help">{help}</div>
          </div>
        ))}
      </section>
    </div>
  );
}
