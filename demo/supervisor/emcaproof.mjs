// emcaproof.mjs — the surface half of M14c, headless.
//
// Same harness as winproof.mjs (Rust kernel core under Node, '#Z' over
// node:fs); what differs is what it watches for.
import { Worker } from "node:worker_threads";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { boot } from "./rustkern.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const rootdir = process.argv[2] ?? join(here, "..", "..", "userspace", "rootfs");
const interactive = process.argv.includes("-i");

function loadSeed(dir, name = "/") {
  const st = fs.statSync(dir);
  if (st.isDirectory())
    return { name, dir: true,
      kids: fs.readdirSync(dir).map((e) => loadSeed(join(dir, e), e)) };
  return { name, dir: false, data: new Uint8Array(fs.readFileSync(dir)) };
}

// '#Z' over node:fs, rooted in a per-run temp dir (the suite's hostfs test)
const zroot = fs.mkdtempSync(join(os.tmpdir(), "ipnx-z-"));
const at = (rel) => {
  const p = rel ? join(zroot, rel) : zroot;
  const probe = fs.existsSync(p) ? p : dirname(p);
  const real = fs.existsSync(probe) ? fs.realpathSync(probe) : null;
  if (!real || !(real === fs.realpathSync(zroot) || real.startsWith(fs.realpathSync(zroot) + "/")))
    throw new Error("path escapes the host root");
  return p;
};
async function hostServe(op) {
  const { kind, path } = op;
  if (kind === 1) {
    const p = at(path);
    try {
      const st = fs.statSync(p);
      return { kind: "meta", dir: st.isDirectory(), len: st.size,
               mtime: Math.floor(st.mtimeMs / 1000), ino: st.ino, mode: st.mode };
    } catch { return { kind: "missing" }; }
  }
  if (kind === 2) {
    const fd = fs.openSync(at(path), "r");
    try {
      const buf = Buffer.alloc(op.n);
      const got = fs.readSync(fd, buf, 0, op.n, op.off);
      return { kind: "bytes", bytes: new Uint8Array(buf.subarray(0, got)) };
    } finally { fs.closeSync(fd); }
  }
  if (kind === 3) {
    const fd = fs.openSync(at(path), "r+");
    try { fs.writeSync(fd, op.data, 0, op.data.length, op.off); }
    finally { fs.closeSync(fd); }
    return { kind: "unit" };
  }
  if (kind === 4) {
    const p = at(path);
    if (op.dir) fs.mkdirSync(p);
    else fs.writeFileSync(p, "");
    fs.chmodSync(p, op.perm);
    return { kind: "unit" };
  }
  if (kind === 5) {
    const p = at(path);
    if (fs.statSync(p).isDirectory()) fs.rmdirSync(p);
    else fs.unlinkSync(p);
    return { kind: "unit" };
  }
  if (kind === 6) {
    const p = at(path);
    if (fs.existsSync(p) && fs.statSync(p).isFile()) fs.truncateSync(p, 0);
    return { kind: "unit" };
  }
  if (kind === 7) {
    const p = at(path);
    const entries = fs.readdirSync(p).sort().map((n) => {
      const st = fs.statSync(join(p, n));
      return { name: n, dir: st.isDirectory(), len: st.size,
               mtime: Math.floor(st.mtimeMs / 1000), ino: st.ino, mode: st.mode };
    });
    return { kind: "entries", entries };
  }
  throw new Error(`hostfs: unknown op ${kind}`);
}

// What this proves is the half rc cannot see: that the chrome emca furnishes
// ARRIVES at a surface. The rc suite tests emca's verbs and its buffer; this
// tests that the toolbar reaching the host is emca's core set merged with the
// type's extras, not either one alone.
//
// PLACEMENT IS NO LONGER PART OF IT (2026-09-01). emca used to place a window by
// writing `pane <name>` to its wctl, and this proof asserted the name arrived.
// Named regions are gone with the compositor redesign; what replaces the
// assertion is M15g, where the surface renders the TREE emca published.
//
// The sibling proof, winproof.mjs, deliberately runs with NO emca at all.
const want = new Map([["ls", "toolbar"], ["edit", "toolbar"]]);
const got = new Map();
let done = false;

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
  winChrome: (wid, ch) => {
    if (done || !want.has(ch.type)) return;
    // Chrome arrives repeatedly as emca furnishes it — a window is minted, then
    // placed, then named, then given its verbs — and each step is one write. So
    // wait for a COMPLETE one rather than latching the first: partial chrome is
    // not a defect, it is the surface seeing the window get built.
    if (!ch.content.trim()) return;   // `ls` declares no verbs of its own, so
                                      // an empty toolbar is the right answer
    if (!got.has(ch.type)) {
      const verbs = ch.toolbar.split("\n").map((l) => l.trim().split(" ")[0]).filter(Boolean);
      got.set(ch.type, { verbs, content: ch.content });
      console.log(`EMCAPROOF host sees: type=${ch.type} content=${ch.content} verbs=${verbs.join(",")}`);
    }
    if (got.size < want.size) return;
    done = true;

    // NOTHING IS UNIVERSAL ON THE TOOLBAR any more (emca.txt): the window
    // operations moved to the title bar row, and Undo belongs to `edit` alone,
    // because Undo is available exactly where a window's operations stay in a
    // buffer emca holds. So what reaches the surface is exactly the TYPE's list.
    const bad = [];
    for (const v of ["Revert", "Undo", "Redo"])
      if (!got.get("edit").verbs.includes(v)) bad.push(`edit: ${v} missing from the toolbar`);
    if (got.get("ls").verbs.length !== 0)
      bad.push(`ls: expected no verbs of its own, got ${got.get("ls").verbs.join(",")}`);
    // Save is NOT there — the file is clean, and Save's presence IS the dirty
    // indicator (acme.c:383's rule, kept, with acme's Put translated)
    if (got.get("edit").verbs.includes("Save"))
      bad.push("edit: Save is in a clean window's toolbar — the dirty indicator lies");

    if (bad.length) {
      console.error("\nEMCAPROOF FAIL:\n  " + bad.join("\n  "));
      process.exit(1);
    }
    console.log("\nEMCAPROOF PASS: the toolbar that reached the surface is exactly the"
      + " TYPE's — nothing is universal there any more — and Save is absent because"
      + " nothing is dirty.");
    process.exit(0);
  },
  error: (text) => console.error(text),
  exit: (code) => process.exit(code),
};

const kernelWasm = fs.readFileSync(join(here, "..", "..", "target",
  "wasm32-unknown-unknown", "release", "browserhost.wasm"));
const { cons } = await boot(host, {
  rootSeed: loadSeed(rootdir), interactive: true, kernelWasm, hostServe,
});

// the DEFAULT WORKSPACE, unmodified: /rc/emca starts emca and opens two typed
// windows. Nothing in this harness knows a type name that /type does not.
cons.feed(new TextEncoder().encode("rc /rc/emca\n"));
setTimeout(() => {
  console.error(`EMCAPROOF FAIL: only ${got.size} of ${want.size} windows were furnished in 20s`);
  process.exit(1);
}, 20000);
