// Serve hosts/web/dist on this machine with the headers a browser wants
// before it gives a page shared memory: COOP and COEP (platforms.md).
//
//   node hosts/web/serve.mjs [port]

import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('dist/', import.meta.url));
const port = Number(process.argv[2] ?? 8080);
const types = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
};

export function serve(p = port) {
  const server = http.createServer((req, res) => {
    let f = path.join(root, path.normalize(decodeURIComponent(new URL(req.url, 'http://here').pathname)));
    if (fs.statSync(f, { throwIfNoEntry: false })?.isDirectory()) f = path.join(f, 'index.html');
    fs.readFile(f, (err, data) => {
      if (err) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, {
        'Content-Type': types[path.extname(f)] ?? 'application/octet-stream',
        'Cross-Origin-Opener-Policy': 'same-origin',
        'Cross-Origin-Embedder-Policy': 'require-corp',
        'Cache-Control': 'no-cache',
      });
      res.end(data);
    });
  });
  return new Promise((r) => server.listen(p, '127.0.0.1', () => r(server)));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const s = await serve();
  console.log(`http://localhost:${s.address().port}/`);
}
