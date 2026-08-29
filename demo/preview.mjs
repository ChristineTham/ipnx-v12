// Local preview of demo/dist with the same COOP/COEP the CDN's _headers set.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, normalize, extname } from "node:path";
const root = join(dirname(fileURLToPath(import.meta.url)), "dist");
const types = { ".html": "text/html", ".mjs": "text/javascript",
  ".js": "text/javascript", ".css": "text/css", ".json": "application/json", ".wasm": "application/wasm" };
createServer(async (req, res) => {
  let p = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (p.endsWith("/")) p += "index.html";
  const f = normalize(join(root, p));
  if (!f.startsWith(root)) { res.writeHead(403).end(); return; }
  try {
    const body = await readFile(f);
    res.writeHead(200, { "Content-Type": types[extname(f)] ?? "application/octet-stream",
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp", "Cache-Control": "no-store" });
    res.end(body);
  } catch { res.writeHead(404).end("not found\n"); }
}).listen(8096, () => console.log("demo preview: http://localhost:8096/"));
