// editor.mjs — the surface's half of a text window (docs/window.md,
// docs/emca.md: "editing is the surface's").
//
// IPNX hands over a PATH; the surface opens it, edits it with a real editor
// component, and streams it back on Put. Everything that is INPUT lives here —
// caret, selection, clipboard, IME, syntax highlighting, folding, multi-cursor,
// find — and none of it is written: it is inherited from CodeMirror, which is
// the founding pattern again (borrow the era's engines through a narrow waist).
//
// What crosses to IPNX is only what IPNX owns: the buffer, as insert/delete
// with a SEQUENCE NUMBER and a HASH per sync so divergence is detectable rather
// than silent; the selection; and the verbs. emca holds the authoritative copy;
// this is the mirror.
//
// The verbs do NOT live here. They are on the window's toolbar, with the tag
// line supplying their argument (emca.md PART FOUR) — which is why this file
// no longer carries a floating bar, a closed verb set, or a snapshotted range.
// What crosses from here is the buffer, the selection, and the keyboard's road
// to the same verbs the toolbar shows.

import { EditorView, minimalSetup } from "../vendor/codemirror.bundle.mjs";

const enc = new TextEncoder();

// the divergence check: cheap, and its absence is the failure nobody notices.
// Over the BYTES, not the code units — the offsets above are byte offsets, and
// an em-dash is one charCode here but three bytes there. Hashing code units
// would false-positive on exactly the content the check exists to protect.
function hash(s) {
  const b = enc.encode(s);
  let h = 0x811c9dc5;
  for (let i = 0; i < b.length; i++) { h ^= b[i]; h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16);
}

// byte offsets are the protocol's coordinate; the editor speaks characters
const boff = (doc, ch) => enc.encode(doc.slice(0, ch)).length;

export function createEditor({ mount, text, send, onPut }) {
  let seq = 0;

  const q = (s) => s.replace(/%/g, "%25").replace(/ /g, "%20").replace(/\n/g, "%0A");

  const view = new EditorView({
    doc: text,
    parent: mount,
    extensions: [
      minimalSetup,
      EditorView.lineWrapping,
      EditorView.theme({
        "&": { height: "100%", fontSize: "13px", background: "#ffffea" },
        ".cm-content": { fontFamily: "ui-monospace, 'SF Mono', Menlo, monospace" },
        "&.cm-focused": { outline: "none" },
      }),
      // the mirror: every change crosses as insert/delete, in order, numbered
      EditorView.updateListener.of((u) => {
        if (u.docChanged) {
          const before = u.startState.doc;
          u.changes.iterChanges((fA, tA, _fB, _tB, ins) => {
            const b0 = enc.encode(before.sliceString(0, fA)).length;
            const b1 = enc.encode(before.sliceString(0, tA)).length;
            if (tA > fA) send(`delete ${b0} ${b1}`);
            const s = ins.toString();
            if (s) send(`insert ${b0} ${q(s)}`);
          });
          seq++;
          // the hash rides every sync: silent divergence is the failure mode
          // worth spending bytes on (docs/window.md)
          send(`seq ${seq} ${hash(view.state.doc.toString())}`);
          send(`dirty 1`);
        }
        if (u.selectionSet && !u.docChanged) {
          const d = u.state.doc.toString();
          const m = u.state.selection.main;
          send(`select ${boff(d, m.from)} ${boff(d, m.to)} ${q(d.slice(m.from, m.to))}`);
        }
      }),

    ],
  });

  // CAPTURE PHASE: this bundle exports no `keymap`/`Prec`, and CodeMirror's own
  // history keymap also listens on keydown. Undo is EMCA's — one stack — so it
  // must be certain to win (docs/window.md).
  view.dom.addEventListener("keydown", (e) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const k = e.key.toLowerCase();
    if (k === "z") { e.preventDefault(); e.stopPropagation(); send(e.shiftKey ? "redo" : "undo"); }
    else if (k === "s") { e.preventDefault(); e.stopPropagation(); onPut?.(view.state.doc.toString()); }
  }, true);


  return {
    view,
    // the keyboard's road to the verbs: the selection, or — acme's own
    // fallback, and the reason ⌘↵ works with no selection at all — the word
    // under the caret.
    rangeVerb: (verb) => {
      const d = view.state.doc.toString();
      const m = view.state.selection.main;
      let from = m.from, to = m.to;
      if (from === to) {
        const w = /[A-Za-z0-9_.\-\/:#$]/;
        while (from > 0 && w.test(d[from - 1])) from--;
        while (to < d.length && w.test(d[to])) to++;
      }
      if (from === to) return;
      send(`${verb} ${boff(d, from)} ${boff(d, to)} ${q(d.slice(from, to))}`);
    },
    text: () => view.state.doc.toString(),
    // the app steers: emca undid something, or Get re-read the file
    setText: (s) => {
      if (s === view.state.doc.toString()) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: s } });
    },
    destroy: () => view.destroy(),
  };
}
