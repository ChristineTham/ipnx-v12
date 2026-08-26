// Static server for the browser port. Exists for one reason: SharedArrayBuffer
// needs cross-origin isolation, so every response carries COOP/COEP.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, normalize, extname } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const port = Number(process.argv[2] ?? 8095);
const types = { ".html": "text/html", ".mjs": "text/javascript",
  ".js": "text/javascript", ".json": "application/json", ".wasm": "application/wasm" };

createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  const file = normalize(join(here, path));
  if (!file.startsWith(here)) { res.writeHead(403).end(); return; }
  try {
    const body = await readFile(file);
    res.writeHead(200, {
      "Content-Type": types[extname(file)] ?? "application/octet-stream",
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cache-Control": "no-store",
    });
    res.end(body);
  } catch {
    res.writeHead(404).end("not found\n");
  }
}).listen(port, () => console.log(`ipnx-v12 poc: http://localhost:${port}/browser/`));
