// Available tracks: everything in assets/tracks/*.json.
// The active track is picked with ?pista=<id> in the URL (the control track by default).

const files = import.meta.glob('../../../assets/tracks/*.json', { query: '?raw', import: 'default', eager: true }) as Record<
  string,
  string
>;

/** A compiler diagnostic attached to a generated track. */
export interface TrackDiagnostic {
  severity: 'info' | 'warning' | 'error';
  code: string;
  message: string;
  at?: { lat: number; lon: number };
}

/** A bundled track and the metadata the lab shows. */
export interface TrackEntry {
  id: string;
  name: string;
  json: string;
  closed: boolean;
  /** Only tracks generated from real data. */
  status?: 'VALID' | 'DEGRADED' | 'UNSUPPORTED';
  stats?: Record<string, number>;
  diagnostics: TrackDiagnostic[];
  attribution?: string;
}

/** Every bundled track, control track first. */
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

/** Track named by `?pista=`, else the control track. */
export function selectedTrack(): TrackEntry {
  const wanted = new URLSearchParams(location.search).get('pista');
  return TRACKS.find((t) => t.id === wanted) ?? TRACKS.find((t) => t.id === 'test-loop') ?? TRACKS[0];
}

/** Reloads the page on another track. */
export function switchTrack(id: string) {
  const url = new URL(location.href);
  url.searchParams.set('pista', id);
  location.assign(url.toString());
}
