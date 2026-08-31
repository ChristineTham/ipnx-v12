// editor.mjs — the surface's half of a text window (docs/window.md,
// docs/emca.txt: "editing is the surface's").
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
// The component criterion the design set (emca.txt): expose the selection, and
// accept custom commands where the user reaches for them. CodeMirror satisfies
// it by using the PLATFORM's context menu, which is where the verbs ride.

import { EditorView, minimalSetup } from "../vendor/codemirror.bundle.mjs";

const enc = new TextEncoder();

// the divergence check: cheap, and its absence is the failure nobody notices
function hash(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16);
}

// byte offsets are the protocol's coordinate; the editor speaks characters
const boff = (doc, ch) => enc.encode(doc.slice(0, ch)).length;

const VERBS = [
  ["Execute", "execute"], ["Look", "look"], ["Pin", "pin"],
  ["Cut", "cut"], ["Copy", "copy"], ["Paste", "paste"],
];

export function createEditor({ mount, text, send, onPut }) {
  let seq = 0;
  let menu = null;

  const closeMenu = () => { menu?.remove(); menu = null; };

  // the verbs, where the platform puts them. CodeMirror has no menu of its own —
  // it uses the browser's — so this IS riding the component's grammar, and it
  // is why property 1 survives a rich editor: any text is still an operand.
  const openMenu = (x, y) => {
    closeMenu();
    const sel = view.state.selection.main;
    const doc = view.state.doc.toString();
    const chosen = doc.slice(sel.from, sel.to);
    menu = document.createElement("div");
    menu.style.cssText = "position:fixed;z-index:9999;background:#fff;color:#123;" +
      "border:1px solid #9aa;border-radius:6px;padding:4px;min-width:120px;" +
      "box-shadow:0 6px 20px rgba(0,0,0,.25);" +
      "font:500 12px/1 'Lucida Grande',system-ui,sans-serif;";
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    for (const [label, verb] of VERBS) {
      // filtered display, closed set — emca.txt's rule for the floating bar
      if (!chosen && (verb === "execute" || verb === "look" || verb === "pin" ||
                      verb === "cut" || verb === "copy")) continue;
      const b = document.createElement("button");
      b.textContent = label;
      b.style.cssText = "display:block;width:100%;text-align:left;border:0;" +
        "background:none;padding:5px 8px;border-radius:4px;cursor:pointer;font:inherit;color:inherit;";
      b.addEventListener("mouseenter", () => { b.style.background = "#eaffff"; });
      b.addEventListener("mouseleave", () => { b.style.background = "none"; });
      b.addEventListener("click", () => {
        closeMenu();
        // the range travels with the verb — snapshotted HERE, because the tap
        // that presses the button collapses the selection first (emca.txt)
        send(`${verb} ${boff(doc, sel.from)} ${boff(doc, sel.to)} ${q(chosen)}`);
      });
      menu.appendChild(b);
    }
    document.body.appendChild(menu);
  };

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
          send(`select ${boff(d, m.from)} ${boff(d, m.to)}`);
        }
      }),
      EditorView.domEventHandlers({
        contextmenu(e) { e.preventDefault(); openMenu(e.clientX, e.clientY); return true; },
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

  document.addEventListener("mousedown", (e) => { if (menu && !menu.contains(e.target)) closeMenu(); });

  return {
    view,
    text: () => view.state.doc.toString(),
    // the app steers: emca undid something, or Get re-read the file
    setText: (s) => {
      if (s === view.state.doc.toString()) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: s } });
    },
    destroy: () => { closeMenu(); view.destroy(); },
  };
}
