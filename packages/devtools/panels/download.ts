/** Saves `data` through a temporary object URL: text as JSON, bytes (`rumbo-replay/2`) as binary. */
export function download(filename: string, data: string | Uint8Array<ArrayBuffer>) {
  const type = typeof data === 'string' ? 'application/json' : 'application/octet-stream';
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
