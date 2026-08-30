// presenter.mjs — the browser surface: the universal SPA of the canvas
// decision, as a standalone artifact. It consumes canvas snapshots and
// emits event lines; it knows nothing of windows, kernels, or the demo
// around it. Any page can:
//     import { createCanvasView } from "./presenter.mjs";
//     const v = createCanvasView({ mount, send, snarf });
//     v.update(snapshot);            // on each sync
// where send(line) delivers an event line to the window's canvas and
// snarf(text) (optional) mirrors clipboard gestures toward /dev/snarf.
// Rendering per docs/canvas.md: stacks are flex (prop=0 hugs, else
// shares), text is text, paths are inline SVG, action roles are real
// links and buttons; edit nodes carry the caret and full editing —
// click positions, arrows navigate, the B1 sweep is the native
// selection (typing replaces, backspace deletes), paste inserts at the
// caret, cmd/ctrl-X cuts, alt/middle-click executes the word or the
// sweep, right-click looks (in-window literal search here; the app
// hears the event too). Renders coalesce on requestAnimationFrame —
// the browser's own credit system.

const cvenc = new TextEncoder();
const cvq = (t) => t.replace(/%/g, "%25").replace(/ /g, "%20").replace(/\n/g, "%0A");
const cvProp = (n) => n.attrs.prop ?? "1";
const cvblen = (t) => cvenc.encode(t).length;
// a byte offset (the protocol's coordinate) back to a char offset (the DOM's)
const cvchar = (t, boff) => {
  let b = 0;
  for (let i = 0; i < t.length; i++) {
    if (b >= boff) return i;
    b += cvenc.encode(t[i]).length;
  }
  return t.length;
};

function cvPlain(el) {
  let t = "";
  for (const nd of el.childNodes) if (nd.nodeType === 3) t += nd.nodeValue;
  return t;
}

function offOf(el, container, o) {
  let x = 0;
  for (const nd of el.childNodes) {
    if (nd === container) return x + o;
    if (nd.nodeType === 3) x += nd.nodeValue.length;
  }
  return x;
}

function caret(v) {
  v.el?.querySelectorAll(".cvcaret").forEach((c) => c.remove());
  const ed = v.edit;
  if (!ed || !ed.el.isConnected) return;
  const t = cvPlain(ed.el);
  let off = v.caretOff ?? t.length;
  if (off > t.length) off = t.length;
  v.caretOff = off;
  const c = document.createElement("span");
  c.className = "cvcaret";
  c.style.cssText = "display:inline-block;width:2px;height:1.05em;background:#000;" +
    "vertical-align:text-bottom;margin:0 -1px;";
  ed.el.replaceChildren(
    document.createTextNode(t.slice(0, off)), c,
    document.createTextNode(t.slice(off)));
}

if (typeof document !== "undefined" && !document.getElementById("cvcaretstyle")) {
  const st = document.createElement("style");
  st.id = "cvcaretstyle";
  // acme's own selection colours (acme.c:862,869 via draw.h): darker
  // yellow in bodies, grey-green in tags — keyed by the node's bg
  st.textContent =
    '[data-cvbg="#ffffea"]::selection,[data-cvbg="#ffffea"] *::selection{background:#eeee9e;}' +
    '[data-cvbg="#eaffff"]::selection,[data-cvbg="#eaffff"] *::selection{background:#9eeeee;}' +
    '.cvscroll::-webkit-scrollbar{width:12px;}' +
    '.cvscroll::-webkit-scrollbar-track{background:#99994c;}' +
    '.cvscroll::-webkit-scrollbar-thumb{background:#ffffea;}';
  document.head.appendChild(st);
}

function render(v, snap) {
  if (!snap) return;
  v.edit = null;
  const kids = new Map();
  const byid = new Map();
  for (const n of snap) byid.set(n.id, n);
  for (const n of snap) {
    if (n.id === 0) continue;
    const par = +(n.attrs.parent ?? 0);
    if (!kids.has(par)) kids.set(par, []);
    kids.get(par).push(n);
  }
  for (const l of kids.values())
    l.sort((a, b) => (+(a.attrs.order ?? 0) - +(b.attrs.order ?? 0)) || (a.id - b.id));

  const build = (n) => {
    let el;
    if (n.kind === "stack") {
      el = document.createElement("div");
      el.style.display = "flex";
      const row = n.attrs.dir === "row";
      el.style.flexDirection = row ? "row" : "column";
      el.style.alignItems = "stretch";
      el.style.minWidth = "0";
      el.style.minHeight = "0";
      if (n.attrs.bg) el.style.background = n.attrs.bg;
      const ks = kids.get(n.id) ?? [];
      ks.forEach((k, i) => {
        const kel = build(k);
        const pr = k.attrs.prop;
        if (row) {
          const rp = pr ?? "1";
          kel.style.flex = rp === "0" ? "0 0 auto" : `${+rp || 1} 1 0`; // share, or hug
        } else if (pr !== undefined) {
          // columns share height only when asked — acme's screen shape
          kel.style.flex = pr === "0" ? "0 0 auto" : `${+pr || 1} 1 0`;
          kel.style.minHeight = "0";
          if (k.kind === "edit" || k.kind === "text") {
            kel.style.overflow = "auto";
            kel.classList.add("cvscroll");
          } else kel.style.overflow = "hidden";
        }
        // hairlines, the reference's black separators: always between a
        // column's children; between a row's unless a side hugs (the box)
        if (i > 0) {
          const prevHug = (ks[i - 1].attrs.prop ?? "1") === "0";
          const curHug = (pr ?? "1") === "0";
          if (!row) kel.style.borderTop = "1px solid #000";
          else if (!prevHug && !curHug) kel.style.borderLeft = "1px solid #000";
        }
        el.appendChild(kel);
      });
    } else if (n.kind === "path") {
      el = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      el.setAttribute("viewBox", (n.attrs.viewbox ?? "0 0 100 100").replace(/"/g, ""));
      el.style.cssText = "width:100%;max-height:300px;";
      const pa = document.createElementNS("http://www.w3.org/2000/svg", "path");
      pa.setAttribute("d", n.data);
      pa.setAttribute("stroke", n.attrs.stroke ?? "#111");
      pa.setAttribute("fill", n.attrs.fill ?? "none");
      pa.setAttribute("stroke-width", n.attrs.width ?? "1.5");
      el.appendChild(pa);
    } else {
      const action = n.attrs.action;
      if (action === "look") { el = document.createElement("a"); el.href = "#"; }
      else if (action === "execute") el = document.createElement("button");
      else el = document.createElement(n.kind === "edit" ? "pre" : "div");
      el.textContent = n.data;
      if (n.attrs.bg) el.style.background = n.attrs.bg;
      if (n.kind === "edit" || n.kind === "text") {
        const wordAt = (e) => {
          const r = document.caretRangeFromPoint?.(e.clientX, e.clientY);
          if (!r || r.startContainer.nodeType !== 3) return null;
          const t = cvPlain(el);
          let off = offOf(el, r.startContainer, r.startOffset);
          if (off > t.length) off = t.length;
          const isw = (c) => c && !/\s/.test(c);
          let a = off, b = off;
          while (a > 0 && isw(t[a - 1])) a--;
          while (b < t.length && isw(t[b])) b++;
          if (a === b) return null;
          return { word: t.slice(a, b), q0: cvblen(t.slice(0, a)), q1: cvblen(t.slice(0, b)) };
        };
        const sweepText = () => {
          const sel = window.getSelection();
          if (!sel || sel.isCollapsed || !el.contains(sel.anchorNode)) return null;
          const t = sel.toString();
          return t.trim().length ? t.trim() : null;
        };
        el.addEventListener("mousedown", (e) => {
          if (e.altKey || e.button === 1) {
            // execute: the swept selection when one exists (arguments and
            // all — the paper's model), else the word under the pointer
            const sw = sweepText();
            const wa = sw ? { word: sw, q0: 0, q1: cvblen(sw) } : wordAt(e);
            if (wa) {
              e.preventDefault();
              e.stopPropagation();
              v.send(`execute ${n.id} ${wa.q0} ${wa.q1} ${cvq(wa.word)}`);
            }
          }
        });
        el.addEventListener("contextmenu", (e) => {
          // B3: look. In-window literal search is the presenter's half;
          // the app hears the event too (paths open windows).
          const wa = wordAt(e);
          if (!wa) return;
          e.preventDefault();
          const t = cvPlain(el);
          const from = (v.edit?.el === el ? (v.caretOff ?? 0) : 0) + 1;
          let at = t.indexOf(wa.word, from);
          if (at < 0) at = t.indexOf(wa.word);
          if (at >= 0 && n.kind === "edit") {
            v.edit = { el, id: n.id };
            v.caretOff = at;
            caret(v);
            el.querySelector(".cvcaret")?.scrollIntoView?.({ block: "center" });
          }
          v.send(`look ${n.id} ${wa.q0} ${wa.q1} ${cvq(wa.word)}`);
        });
      }
      if (n.kind === "edit" || n.kind === "text")
        el.dataset.cvbg = n.attrs.bg ?? "";
      if (n.kind === "edit") {
        el.style.cssText = "white-space:pre-wrap;margin:0;outline:none;min-height:1.4em;" +
          "overflow-wrap:anywhere;min-width:0;padding:0 1px 0 3px;" +
          "font:500 14px/1.3 'Lucida Grande','Lucida Sans Unicode',system-ui,sans-serif;" +
          (n.attrs.bg ? "background:" + n.attrs.bg + ";" : "");
        el.dataset.cvid = n.id;                      // reportSel finds the node
        v.edit = { el, id: n.id };                   // the delegated keyboard's target
        el.addEventListener("mousedown", (e) => {
          e.stopPropagation();
          v.edit = { el, id: n.id };
          // record the caret target but do NOT repaint the DOM here — a
          // rebuild mid-gesture destroys the browser's selection anchor,
          // killing sweeps and double-click word selection (measured)
          const r = document.caretRangeFromPoint?.(e.clientX, e.clientY);
          v._pendOff = (r && r.startContainer.nodeType === 3 && r.startContainer.parentNode === el)
            ? offOf(el, r.startContainer, r.startOffset)
            : cvPlain(el).length;
        });
      }
      if (action)
        el.addEventListener("click", (e) => {
          e.preventDefault();
          v.send(`${action} ${n.id} 0 ${cvblen(n.data)} ${cvq(n.data)}`);
        });
    }
    return el;
  };
  const root = byid.get(0);
  const rootEl = root ? build(root) : document.createTextNode("");
  if (root) { rootEl.style.minHeight = "100%"; rootEl.style.height = "100%"; }
  v.el.replaceChildren(rootEl);

  // the app steered the selection: sel=q0,q1 on an edit node (byte
  // offsets). Scroll it into view always; take the keyboard only when
  // the node is new this render — output tails never steal the caret.
  for (const n of snap) {
    if (n.kind !== "edit" || n.attrs.sel === undefined) continue;
    if (v._appliedSel.get(n.id) === n.attrs.sel) continue;
    v._appliedSel.set(n.id, n.attrs.sel);
    const el = v.el.querySelector(`[data-cvid="${n.id}"]`);
    if (!el) continue;
    const [bq0, bq1] = n.attrs.sel.split(",").map((x) => +x || 0);
    const fresh = !v._seen.has(n.id);
    if (fresh || v.edit?.id === n.id) {
      v.edit = { el, id: n.id };
      v.caretOff = cvchar(n.data, bq1);
    }
    el._cvsel = { a: cvchar(n.data, bq0), b: cvchar(n.data, bq1) };
    el._cvscroll = true;
  }
  const live = new Set(snap.map((n) => n.id));
  for (const id of v._seen)
    if (!live.has(id)) { v._seen.delete(id); v._appliedSel.delete(id); }
  for (const n of snap) v._seen.add(n.id);
  caret(v);
  for (const el of v.el.querySelectorAll("[data-cvid]")) {
    if (!el._cvscroll) continue;
    el._cvscroll = false;
    const c = el.querySelector(".cvcaret");
    (c ?? el).scrollIntoView?.({ block: "nearest" });
    const s = el._cvsel;
    if (s && s.a !== s.b && v.edit?.el === el) {
      // paint the range as the native selection
      const r = document.createRange();
      let x = 0, done = 0;
      for (const nd of el.childNodes) {
        if (nd.nodeType !== 3) continue;
        const ln = nd.nodeValue.length;
        if (!(done & 1) && s.a <= x + ln) { r.setStart(nd, Math.max(0, s.a - x)); done |= 1; }
        if (!(done & 2) && s.b <= x + ln) { r.setEnd(nd, Math.max(0, s.b - x)); done |= 2; break; }
        x += ln;
      }
      if (done === 3) {
        const sel = window.getSelection();
        sel.removeAllRanges();
        sel.addRange(r);
      }
    }
    el._cvsel = null;
  }
}

// the surface's half of the select event: report the user's selection
// (or collapsed caret) in an edit node when it changes
function reportSel(v) {
  let id = null, a = 0, b = 0;
  const sel = window.getSelection();
  if (sel && !sel.isCollapsed && sel.rangeCount > 0) {
    const r = sel.getRangeAt(0);
    const el = r.startContainer.parentElement?.closest?.("[data-cvid]");
    if (el && v.el.contains(el) && el.contains(r.endContainer)) {
      id = +el.dataset.cvid;
      const t = cvPlain(el);
      a = offOf(el, r.startContainer, r.startOffset);
      b = offOf(el, r.endContainer, r.endOffset);
      if (a > b) [a, b] = [b, a];
      a = cvblen(t.slice(0, a));
      b = cvblen(t.slice(0, b));
    }
  }
  if (id === null && v.edit) {
    id = v.edit.id;
    const t = cvPlain(v.edit.el);
    const off = Math.min(v.caretOff ?? t.length, t.length);
    a = b = cvblen(t.slice(0, off));
  }
  if (id === null) return;
  const key = `${id} ${a} ${b}`;
  if (v._lastSel === key) return;
  v._lastSel = key;
  v.send(`select ${id} ${a} ${b}`);
}

function installHandlers(v) {
  v.el.addEventListener("mouseup", () => {
    // the caret paints at gesture end, and only when no sweep formed
    const sel = window.getSelection();
    if (v._pendOff !== undefined && v.edit && (!sel || sel.isCollapsed)) {
      v.caretOff = v._pendOff;
      caret(v);
    }
    v._pendOff = undefined;
    reportSel(v);
  });
  // when the native selection collapses (any click, anywhere), forget
  // which sel values were applied — a repeated look re-highlights
  document.addEventListener("selectionchange", () => {
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) v._appliedSel.clear();
  });
  v.el.addEventListener("copy", () => {
    const t = window.getSelection()?.toString() ?? "";
    if (t) v.snarf?.(t);                             // /dev/snarf hears the gesture
  });
  v.el.addEventListener("paste", (e) => {
    const ed = v.edit;
    if (!ed) return;
    const txt = e.clipboardData?.getData("text/plain") ?? "";
    if (!txt) return;
    e.preventDefault();
    v.snarf?.(txt);                                  // pasted text is the snarf now
    const t = cvPlain(ed.el);
    const off = Math.min(v.caretOff ?? t.length, t.length);
    const q0 = cvblen(t.slice(0, off));
    v.caretOff = off + txt.length;
    ed.el.textContent = t.slice(0, off) + txt + t.slice(off);
    caret(v);
    v.send(`insert ${ed.id} ${q0} ${cvq(txt)}`);
  });
  v.el.addEventListener("keydown", (e) => {
    const ed = v.edit;
    if (!ed) return;
    if ((e.metaKey || e.ctrlKey) && (e.key === "x" || e.key === "X")) {
      // cut: the native clipboard IS snarf — copy the sweep, delete it
      const t0 = cvPlain(ed.el);
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed) return;
      const stext = sel.toString();
      e.preventDefault();
      try { navigator.clipboard?.writeText(stext); } catch {}
      v.snarf?.(stext);
      const r0 = sel.getRangeAt(0);
      let a = offOf(ed.el, r0.startContainer, r0.startOffset);
      let b = offOf(ed.el, r0.endContainer, r0.endOffset);
      if (a > b) [a, b] = [b, a];
      if (a === b) return;
      const q0 = cvblen(t0.slice(0, a));
      const q1 = cvblen(t0.slice(0, b));
      v.send(`delete ${ed.id} ${q0} ${q1}`);
      sel.removeAllRanges();
      v.caretOff = a;
      ed.el.textContent = t0.slice(0, a) + t0.slice(b);
      caret(v);
      return;
    }
    if (e.metaKey || e.ctrlKey) return;
    const t = cvPlain(ed.el);
    let off = Math.min(v.caretOff ?? t.length, t.length);
    // the B1 sweep is the native selection: typing replaces it,
    // backspace deletes it — the input convention's promise
    const selRange = () => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || sel.rangeCount === 0) return null;
      const r = sel.getRangeAt(0);
      if (!ed.el.contains(r.startContainer) || !ed.el.contains(r.endContainer)) return null;
      let a = offOf(ed.el, r.startContainer, r.startOffset);
      let b = offOf(ed.el, r.endContainer, r.endOffset);
      if (a > b) [a, b] = [b, a];
      return a === b ? null : { a, b };
    };
    const delSel = (sr) => {
      const q0 = cvblen(t.slice(0, sr.a));
      const q1 = cvblen(t.slice(0, sr.b));
      v.send(`delete ${ed.id} ${q0} ${q1}`);
      window.getSelection()?.removeAllRanges();
      return t.slice(0, sr.a) + t.slice(sr.b);
    };
    const setText = (nt, noff) => {
      v.caretOff = noff;
      ed.el.textContent = nt;
      caret(v);
    };
    const lineNav = (dir) => {
      const ls = t.slice(0, off).split("\n");
      const row = ls.length - 1;
      const col = ls[row].length;
      const all = t.split("\n");
      const nr = row + dir;
      if (nr < 0 || nr >= all.length) return off;
      let base = 0;
      for (let i = 0; i < nr; i++) base += all[i].length + 1;
      return base + Math.min(col, all[nr].length);
    };
    if (e.key === "ArrowLeft") { e.preventDefault(); v.caretOff = Math.max(0, off - 1); caret(v); return; }
    if (e.key === "ArrowRight") { e.preventDefault(); v.caretOff = Math.min(t.length, off + 1); caret(v); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); v.caretOff = lineNav(-1); caret(v); return; }
    if (e.key === "ArrowDown") { e.preventDefault(); v.caretOff = lineNav(1); caret(v); return; }
    if (e.key === "Home") { e.preventDefault(); v.caretOff = t.lastIndexOf("\n", off - 1) + 1; caret(v); return; }
    if (e.key === "End") { e.preventDefault(); const nx = t.indexOf("\n", off); v.caretOff = nx < 0 ? t.length : nx; caret(v); return; }
    if (e.key === "Backspace") {
      e.preventDefault();
      const sr = selRange();
      if (sr) { setText(delSel(sr), sr.a); return; }
      if (off === 0) return;
      const q0 = cvblen(t.slice(0, off - 1));
      const q1 = cvblen(t.slice(0, off));
      setText(t.slice(0, off - 1) + t.slice(off), off - 1);      // local echo
      v.send(`delete ${ed.id} ${q0} ${q1}`);
      return;
    }
    let ch = null;
    if (e.key === "Enter") ch = "\n";
    else if (e.key.length === 1) ch = e.key;
    if (ch === null) return;
    e.preventDefault();
    let t2 = t;
    let off2 = off;
    const sr = selRange();
    if (sr) { t2 = delSel(sr); off2 = sr.a; }
    const q0 = cvblen(t2.slice(0, off2));
    setText(t2.slice(0, off2) + ch + t2.slice(off2), off2 + 1);  // local echo
    v.send(`insert ${ed.id} ${q0} ${cvq(ch)}`);
    reportSel(v);
  });
}

export function createCanvasView({ mount, send, snarf }) {
  const v = { el: null, edit: null, caretOff: null, send, snarf, _snap: null, _raf: 0,
    _appliedSel: new Map(), _seen: new Set(), _lastSel: "" };
  v.el = document.createElement("div");
  v.el.tabIndex = 0;
  v.el.style.cssText = "position:absolute;left:0;right:0;bottom:0;top:30px;overflow:auto;" +
    "background:#fff;color:#111;outline:none;" +
    "-webkit-user-select:text;user-select:text;" +   // the chrome's none stops here
    "font:500 14px/1.3 'Lucida Grande','Lucida Sans Unicode',system-ui,sans-serif;";
  mount.appendChild(v.el);
  setTimeout(() => v.el.focus(), 0);                 // a new view takes the keyboard
  installHandlers(v);
  return {
    el: v.el,
    focus: () => v.el.focus(),
    update: (snap) => {                              // rAF: the browser's own credit
      v._snap = snap;
      if (!v._raf) {
        v._raf = requestAnimationFrame(() => {
          v._raf = 0;
          render(v, v._snap);
        });
      }
    },
  };
}
