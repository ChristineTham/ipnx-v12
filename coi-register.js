// Registers coi-sw.js and brings the page up cross-origin isolated.
// The dance, third revision (each step measured):
//  - already isolated, or no SW support: do nothing.
//  - not yet controlled: register; reload when the worker takes control
//    (clients.claim() fires controllerchange). A hard reload leaves the
//    registration active but the page uncontrolled and controllerchange
//    silent — one latched plain reload restores control.
//  - controlled but NOT isolated: usually a WebKit race (control arrived,
//    the header-injected response did not — Safari landed here in the
//    field, 2026-08-29, on a load earlier measurements passed), so retry
//    ONCE, latched; only then surrender to the page's guard. Storage is
//    try/caught throughout: Safari private mode may refuse it, and the
//    latch then lives in memory (still loop-safe for that tab).
(() => {
  if (crossOriginIsolated || !("serviceWorker" in navigator)) return;
  let mem = {};
  const get = (k) => { try { return sessionStorage.getItem(k); } catch (_) { return mem[k] ?? null; } };
  const set = (k, v) => { try { sessionStorage.setItem(k, v); } catch (_) { mem[k] = v; } };
  const del = (k) => { try { sessionStorage.removeItem(k); } catch (_) { delete mem[k]; } };
  if (navigator.serviceWorker.controller) {
    if (!get("coi-retry2")) {                       // the WebKit race: bounded
      set("coi-retry2", "1");
      location.reload();
    }
    return;                                         // retried already: the guard speaks
  }
  del("coi-retry2");
  const sw = new URL("coi-sw.js", document.currentScript.src);
  navigator.serviceWorker.addEventListener("controllerchange",
    () => location.reload(), { once: true });
  navigator.serviceWorker.register(sw, { scope: new URL("./", sw).pathname })
    .then(async () => {
      // hard reload: registration active, page uncontrolled, controllerchange
      // never fires — one plain reload restores control (latched)
      await navigator.serviceWorker.ready;
      if (!navigator.serviceWorker.controller && !get("coi-retry")) {
        set("coi-retry", "1");
        location.reload();
      } else del("coi-retry");
    })
    .catch((e) => console.warn("coi: registration failed", e));
})();
