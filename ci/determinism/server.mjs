// Servidor estático mínimo sobre la raíz del repo, más /__cases.json y la página del arnés.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { join, normalize, extname } from 'node:path';

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.wasm': 'application/wasm' };

export function serve(root, cases, page) {
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    try {
      if (path === '/__cases.json') {
        res.writeHead(200, { 'content-type': TYPES['.json'] }).end(JSON.stringify(cases));
        return;
      }
      const file = path === '/' ? page : join(root, normalize(path).replace(/^([/\\])+/, ''));
      if (!file.startsWith(root) && file !== page) throw new Error('fuera de la raíz');
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream', 'cache-control': 'no-store' }).end(body);
    } catch {
      res.writeHead(404).end('no encontrado');
    }
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve({ url: `http://127.0.0.1:${server.address().port}/`, close: () => server.close() }));
  });
}
