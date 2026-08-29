// Cross-origin-isolation service worker: GitHub Pages cannot set response
// headers, so this worker adds the COOP/COEP pair SharedArrayBuffer needs to
// every same-origin response. Hosts that set real headers (_headers on
// Netlify-class CDNs, preview.mjs locally) never register it.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", (e) => {
  const r = e.request;
  if (r.cache === "only-if-cached" && r.mode !== "same-origin") return;
  e.respondWith(fetch(r).then((resp) => {
    if (resp.status === 0 || resp.type === "opaque") return resp;
    const h = new Headers(resp.headers);
    h.set("Cross-Origin-Embedder-Policy", "require-corp");
    h.set("Cross-Origin-Opener-Policy", "same-origin");
    return new Response(resp.body, { status: resp.status, statusText: resp.statusText, headers: h });
  }));
});
