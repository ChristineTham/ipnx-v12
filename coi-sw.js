// v3 — Cross-origin-isolation service worker.
// Navigations get COOP/COEP stamped from the network (documents are where
// isolation lives). Same-origin subresources — worker module graphs above
// all — are served from CacheStorage with COEP stamped: dedicated workers in
// a require-corp world need their own script responses to carry COEP, and
// caching sidesteps WebKit's flaky repeated SW re-streaming (an opaque
// generic Event at the eighth worker, measured 2026-08-29).
const CACHE = "coi-1788128781";
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(
  caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
    .then(() => self.clients.claim())));
const stamp = (resp) => {
  if (resp.status === 0 || resp.type === "opaque") return resp;
  const h = new Headers(resp.headers);
  h.set("Cross-Origin-Embedder-Policy", "require-corp");
  h.set("Cross-Origin-Opener-Policy", "same-origin");
  return new Response(resp.body, { status: resp.status, statusText: resp.statusText, headers: h });
};
self.addEventListener("fetch", (e) => {
  const r = e.request;
  // no-cache: revalidate with the CDN (fast 304s) so a deploy reaches every
  // visitor on one plain reload — the HTTP cache never pins a stale build
  if (r.mode === "navigate") { e.respondWith(fetch(r, { cache: "no-cache" }).then(stamp)); return; }
  const u = new URL(r.url);
  if (u.origin !== self.location.origin || r.method !== "GET") return;
  // the big manifests stream straight from the network: same-origin needs no
  // COEP stamp, ?v= busting handles freshness, and buffering 46-80MB inside
  // the SW kills the page's download progress (measured: an opaque stall)
  if (u.pathname.includes("/build/")) return;
  e.respondWith((async () => {
    const c = await caches.open(CACHE);
    const hit = await c.match(r);
    if (hit) return hit;
    const resp = await fetch(r, { cache: "no-cache" });
    const stamped = stamp(resp);
    if (stamped.status === 200) {
      const copy = stamped.clone();
      const buf = await copy.arrayBuffer();          // settle fully before caching
      await c.put(r, new Response(buf, { status: 200, headers: copy.headers }));
      return new Response(buf, { status: 200, headers: stamped.headers });
    }
    return stamped;
  })());
});
