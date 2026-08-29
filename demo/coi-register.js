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
  const sw = new URL("coi-sw.js", document.currentScript.src);
  navigator.serviceWorker.addEventListener("controllerchange",
    () => location.reload(), { once: true });
  navigator.serviceWorker.register(sw, { scope: new URL("./", sw).pathname })
    .then(async (reg) => {
      // a hard reload bypasses the SW: the registration is active but the
      // page is uncontrolled and controllerchange will never fire — one
      // plain reload restores control (one-shot; the guard catches the rest)
      await navigator.serviceWorker.ready;
      if (!navigator.serviceWorker.controller && !sessionStorage.getItem("coi-retry")) {
        sessionStorage.setItem("coi-retry", "1");
        location.reload();
      } else sessionStorage.removeItem("coi-retry");
    })
    .catch((e) => console.warn("coi: registration failed", e));
})();
