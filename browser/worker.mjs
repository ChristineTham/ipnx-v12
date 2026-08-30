self.addEventListener("error", (e) => { try { self.postMessage({ __dbg: (e.message||"?") + " @" + (e.filename||"?") + ":" + (e.lineno||"?") + " | " + (e.error && e.error.stack || "") }); } catch (_) {} });
self.addEventListener("unhandledrejection", (e) => { try { self.postMessage({ __dbg: "rejection: " + (e.reason && (e.reason.stack || e.reason.message) || String(e.reason)) }); } catch (_) {} });
// Browser shim for the guest runner: the message port over the Worker global.
import { startGuest } from "../supervisor/guestcore.mjs";

startGuest({
  post: (m, transfer = []) => self.postMessage(m, transfer),
  onMessage: (cb) => self.addEventListener("message", (e) => cb(e.data)),
});
