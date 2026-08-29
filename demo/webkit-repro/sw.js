// Minimal COI-style SW: stamp headers on everything, no cache — the
// re-streaming path the demo's first SW used.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  e.respondWith(fetch(e.request).then((r) => {
    if (r.status === 0 || r.type === "opaque") return r;
    const h = new Headers(r.headers);
    h.set("Cross-Origin-Embedder-Policy", "require-corp");
    h.set("Cross-Origin-Opener-Policy", "same-origin");
    return new Response(r.body, { status: r.status, statusText: r.statusText, headers: h });
  }));
});
