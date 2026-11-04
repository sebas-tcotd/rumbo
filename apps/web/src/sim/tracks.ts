// Pistas disponibles: todo lo que haya en assets/tracks/*.json.
// La pista activa se elige con ?pista=<id> en la URL (por defecto, la pista de control).

const files = import.meta.glob('../../../../assets/tracks/*.json', { query: '?raw', import: 'default', eager: true }) as Record<
  string,
  string
>;

export interface TrackDiagnostic {
  severity: 'info' | 'warning' | 'error';
  code: string;
  message: string;
  at?: { lat: number; lon: number };
}

export interface TrackEntry {
  id: string;
  name: string;
  json: string;
  closed: boolean;
  /** Solo pistas generadas desde datos reales. */
  status?: 'VALID' | 'DEGRADED' | 'UNSUPPORTED';
  stats?: Record<string, number>;
  diagnostics: TrackDiagnostic[];
  attribution?: string;
}

export const TRACKS: TrackEntry[] = Object.values(files)
  .map((json) => {
    const t = JSON.parse(json);
    return {
      id: t.id,
      name: t.name,
      json,
      closed: t.closed,
      status: t.status,
      stats: t.stats,
      diagnostics: t.diagnostics ?? [],
      attribution: t.source?.attribution,
    } as TrackEntry;
  })
  .sort((a, b) => (a.id === 'test-loop' ? -1 : b.id === 'test-loop' ? 1 : a.name.localeCompare(b.name)));

export function selectedTrack(): TrackEntry {
  const wanted = new URLSearchParams(location.search).get('pista');
  return TRACKS.find((t) => t.id === wanted) ?? TRACKS.find((t) => t.id === 'test-loop') ?? TRACKS[0];
}

export function switchTrack(id: string) {
  const url = new URL(location.href);
  url.searchParams.set('pista', id);
  location.assign(url.toString());
}
