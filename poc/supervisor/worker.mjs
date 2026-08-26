// Node shim for the guest runner: the message port over worker_threads.
import { parentPort } from "node:worker_threads";
import { startGuest } from "./guestcore.mjs";

startGuest({
  post: (m, transfer = []) => parentPort.postMessage(m, transfer),
  onMessage: (cb) => parentPort.on("message", cb),
});
