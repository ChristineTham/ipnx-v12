// Browser shim for the guest runner: the message port over the Worker global.
import { startGuest } from "../supervisor/guestcore.mjs";

startGuest({
  post: (m, transfer = []) => self.postMessage(m, transfer),
  onMessage: (cb) => self.addEventListener("message", (e) => cb(e.data)),
});
