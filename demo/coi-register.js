// Registers coi-sw.js and reloads once so the page comes back isolated.
// No-op when the host already serves the headers (crossOriginIsolated true),
// and a sessionStorage guard prevents a reload loop if isolation never takes.
(() => {
  if (crossOriginIsolated || !("serviceWorker" in navigator)) return;
  if (sessionStorage.getItem("coi-reloaded")) return;
  const sw = new URL("coi-sw.js", document.currentScript.src);
  navigator.serviceWorker.register(sw, { scope: new URL("./", sw).pathname })
    .then(() => navigator.serviceWorker.ready)
    .then(() => { sessionStorage.setItem("coi-reloaded", "1"); location.reload(); })
    .catch((e) => console.warn("coi: registration failed", e));
})();
