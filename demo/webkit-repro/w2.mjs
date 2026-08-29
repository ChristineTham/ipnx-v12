import { x } from "./dep.mjs";
onmessage = (e) => {
  const d = e.data;
  if (d.sab) {
    const a = new Int32Array(d.sab);
    Atomics.wait(a, 0, 0, d.hold);        // park like a guest on its mailbox
  }
  postMessage("ok" + x);
};
postMessage("up");
