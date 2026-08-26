// Node host for the hosted kernel: filesystem seeding, worker_threads,
// stdio. The kernel itself (kernel.mjs) is platform-neutral; the browser
// host is ../browser/main.mjs.
import { Worker } from "node:worker_threads";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { boot } from "./kernel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const rootdir = process.argv[2] ?? join(here, "..", "rootfs");
const interactive = process.argv.includes("-i");

function loadSeed(dir, name = "/") {
  const st = statSync(dir);
  if (st.isDirectory())
    return { name, dir: true,
      kids: readdirSync(dir).map((e) => loadSeed(join(dir, e), e)) };
  return { name, dir: false, data: new Uint8Array(readFileSync(dir)) };
}

const host = {
  spawnWorker: (initMsg, transfer) => {
    const w = new Worker(join(here, "worker.mjs"));
    let handler = () => {};
    let errh = () => {};
    w.on("message", (m) => handler(m));
    w.on("error", (e) => errh(e));
    w.postMessage(initMsg, transfer);
    return {
      post: (m, t = []) => w.postMessage(m, t),
      setHandler: (cb) => { handler = cb; },
      onMessage: (cb) => { handler = cb; },
      onError: (cb) => { errh = cb; },
      terminate: () => w.terminate(),
    };
  },
  consWrite: (bytes) => process.stdout.write(bytes),
  error: (text) => console.error(text),
  exit: (code) => process.exit(code),
};

const { cons } = await boot(host, { rootSeed: loadSeed(rootdir), interactive });
if (interactive) {
  process.stdin.on("data", (c) => cons.feed(new Uint8Array(c)));
  process.stdin.on("end", () => cons.end());
}
