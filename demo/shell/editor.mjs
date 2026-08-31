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

// THE CLOSED VERB SET (emca.txt, "The floating bar"). Closed because the verbs
// that take a RANGE operand are fixed by acme's layer 3, not by the user — the
// open half is the command set reached through Execute, never the bar.
//
// `Look` is absent on purpose, and its absence is the point: acme's look parses
// your text and silently picks one of open / jump / search, which is exactly why
// nobody can predict a right-click. Here the parsing is unchanged and it
// POPULATES the bar instead of deciding — so the choice is shown.
//
// Order is canonical and fixed, so a button never moves under a finger. The two
// conditional verbs lead, because they are the specific ones and because that is
// where the bar grows when emca answers.
const BAR = ["Open", "Jump", "Search", "Execute", "Cut", "Copy", "Paste", "Pin", "Edit"];
const ALWAYS = new Set(["Search", "Execute", "Cut", "Copy", "Paste", "Pin", "Edit"]);
// Splitting look into Open/Jump/Search RE-DIVIDES THE LABOUR, and the division is
// the same one the toolbar already draws between ipnx: and host:. Open is IPNX's
// — it mints a window and resolves against the namespace. Jump and Search are
// pure rendering and input inside a buffer this side already holds, so sending
// them down would be a round trip to accomplish nothing.
const HOSTSIDE = new Set(["Cut", "Copy", "Paste", "Jump", "Search"]);

export function createEditor({ mount, text, send, onPut }) {
  let seq = 0;
  let menu = null;
  let shot = null;   // THE SNAPSHOT — see openMenu

  const closeMenu = () => { menu?.remove(); menu = null; shot = null; };

  // THE FLOATING BAR. Its operand is a range of text, so by
  // operand-determines-surface it sits AT THE TEXT — position encodes what the
  // verbs act on, which is the discoverability acme never had, delivered by
  // geometry instead of documentation.
  const openMenu = (rect, applicable) => {
    const had = menu;
    menu?.remove();
    menu = document.createElement("div");
    menu.className = "fbar";
    menu.setAttribute("role", "toolbar");
    menu.setAttribute("aria-label", "verbs for the selection");
    for (const label of BAR) {
      if (!applicable.has(label)) continue;
      const b = document.createElement("button");
      b.textContent = label;
      b.tabIndex = 0;
      b.addEventListener("click", () => {
        const s = shot;                     // the SNAPSHOT, not the live selection
        closeMenu();
        if (!s) return;
        if (HOSTSIDE.has(label)) { host(label, s); return; }
        send(`${label.toLowerCase()} ${s.from} ${s.to} ${q(s.text)}`);
      });
      menu.appendChild(b);
    }
    document.body.appendChild(menu);
    // clamp into the viewport; the bar sits above the range, below it if there
    // is no room, which is the only case where "at the text" must bend
    const w = menu.offsetWidth, h = menu.offsetHeight;
    let x = Math.min(Math.max(4, rect.left), innerWidth - w - 4);
    let y = rect.top - h - 6;
    if (y < 4) y = rect.bottom + 6;
    menu.style.left = `${x}px`;
    menu.style.top = `${y}px`;
    if (!had) menu.animate?.([{ opacity: 0 }, { opacity: 1 }], { duration: 90 });
  };

  // Cut / Copy / Paste are the PLATFORM's — they carry the system clipboard,
  // IME and permissions, and routing them through IPNX would only lose that.
  // /dev/snarf is the sync point, which the surface writes on its own.
  const host = (verb, s) => {
    if (verb === "Copy" || verb === "Cut") {
      navigator.clipboard?.writeText(s.text);
      send(`snarf ${q(s.text)}`);            // /dev/snarf is the sync point
      if (verb === "Cut") view.dispatch({ changes: { from: s.cfrom, to: s.cto, insert: "" } });
      return;
    }
    if (verb === "Paste") {
      navigator.clipboard?.readText?.().then((txt) => {
        if (txt) view.dispatch({ changes: { from: s.cfrom, to: s.cto, insert: txt } });
      });
      return;
    }
    // JUMP: sam's address forms, applied to this buffer. emca decided the range
    // IS an address; where it lands is the surface's business.
    if (verb === "Jump") {
      const d = view.state.doc;
      let a = s.text.trim();
      if (a[0] === ":") a = a.slice(1);
      let pos = null;
      if (a === "$") pos = d.length;
      else if (a[0] === "#") pos = Math.min(d.length, Math.max(0, +a.slice(1) || 0));
      else if (/^\d+$/.test(a)) pos = d.line(Math.min(d.lines, Math.max(1, +a))).from;
      else if (a.length >= 3 && a[0] === "/" && a[a.length - 1] === "/") {
        try {
          const m = new RegExp(a.slice(1, -1)).exec(d.toString().slice(s.cto));
          if (m) pos = s.cto + m.index;
        } catch { /* not a regexp after all — no jump, and no crash */ }
      }
      if (pos !== null) reveal(pos);
      return;
    }
    // SEARCH: the next occurrence of the range, wrapping. minimalSetup ships no
    // search panel, and find-in-file proper is the component's to grow into.
    if (verb === "Search") {
      const d = view.state.doc.toString();
      let i = d.indexOf(s.text, s.cto);
      if (i < 0) i = d.indexOf(s.text);
      if (i >= 0) reveal(i, i + s.text.length);
    }
  };

  const reveal = (from, to = from) => {
    view.dispatch({ selection: { anchor: from, head: to },
                    scrollIntoView: true });
    view.focus();
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
        if (u.selectionSet || u.docChanged) {
          const d = u.state.doc.toString();
          const m = u.state.selection.main;
          if (m.empty) { closeMenu(); send(`select ${boff(d, m.from)} ${boff(d, m.to)} `); return; }
          // TIMING, THE KNOWN TRAP (emca.txt): the tap that presses a button
          // collapses the native selection first, so the range is snapshotted
          // HERE, when the bar appears — not read when it is used. Underneath
          // is a conflation: dot is insertion point, operand and clipboard
          // source at once, three lifetimes that a mouse hides and touch cannot.
          shot = { from: boff(d, m.from), to: boff(d, m.to),
                   cfrom: m.from, cto: m.to, text: d.slice(m.from, m.to) };
          send(`select ${shot.from} ${shot.to} ${q(shot.text)}`);
          // ALWAYS-applicable verbs render immediately; Open and Jump appear
          // when emca answers. The bar never waits on a round trip.
          const r = view.coordsAtPos(m.from), r2 = view.coordsAtPos(m.to);
          if (r) openMenu({ left: Math.min(r.left, r2?.left ?? r.left), top: r.top,
                            bottom: Math.max(r.bottom, r2?.bottom ?? r.bottom) }, ALWAYS);
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

  document.addEventListener("mousedown", (e) => { if (menu && !menu.contains(e.target)) closeMenu(); });

  return {
    view,
    // emca answered the select event with which of the closed set applies;
    // the bar grows to match. Ignored if the selection has since gone.
    setVerbs: (list) => {
      if (!shot || !menu) return;
      const set = new Set(list.split("\n").map((s) => s.trim()).filter(Boolean));
      if (!set.size) return;
      for (const a of ALWAYS) set.add(a);
      const r = view.coordsAtPos(view.state.selection.main.from);
      if (r) openMenu({ left: r.left, top: r.top, bottom: r.bottom }, set);
    },
    closeBar: closeMenu,
    // the keyboard's road to the range verbs: the selection, or — acme's own
    // fallback, and the reason ⌘↵ works with no selection at all — the word
    // under the caret. Same event as the bar's button, same snapshot rule.
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
      closeMenu();
      send(`${verb} ${boff(d, from)} ${boff(d, to)} ${q(d.slice(from, to))}`);
    },
    text: () => view.state.doc.toString(),
    // the app steers: emca undid something, or Get re-read the file
    setText: (s) => {
      if (s === view.state.doc.toString()) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: s } });
    },
    destroy: () => { closeMenu(); view.destroy(); },
  };
}
