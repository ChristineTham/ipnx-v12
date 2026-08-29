// Registers coi-sw.js and brings the page up cross-origin isolated.
// The dance, done properly (the first version reloaded on `ready` and set a
// session latch — that races slower controller takeover, e.g. WebKit's, and
// then never retries; measured on Safari 2026-08-29):
//  - already isolated, or no SW support: do nothing.
//  - controlled but NOT isolated: the worker's headers genuinely don't take
//    on this engine — stop; the page's own guard explains. (Loop-safe.)
//  - not yet controlled: register, and reload exactly when the worker takes
//    control (clients.claim() fires controllerchange) — the reloaded page is
//    then served through the worker, headers and all.
(() => {
  if (crossOriginIsolated || !("serviceWorker" in navigator)) return;
  if (navigator.serviceWorker.controller) return;
  navigator.serviceWorker.addEventListener("controllerchange",
    () => location.reload(), { once: true });
  const sw = new URL("coi-sw.js", document.currentScript.src);
  navigator.serviceWorker.register(sw, { scope: new URL("./", sw).pathname })
    .catch((e) => console.warn("coi: registration failed", e));
})();
