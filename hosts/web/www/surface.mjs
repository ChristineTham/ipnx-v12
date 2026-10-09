// The surface — what a person sees and does (docs/surface.md): emca's tree
// drawn in a page, every window's chrome, its text, and the input. emca's
// model is emca.mjs; this draws it and hands it what was done, and decides
// nothing about what anything is or means.
//
// What each window shows, top to bottom (emca.md, the agreed design, 5):
//
//   title and window controls
//   tag line and toolbar
//   content
//   status line
//
// and the outermost window's chrome is the page's own: a global toolbar
// with the system's verbs (surface.md, The outermost window's chrome).

import { EditorView, minimalSetup } from './vendor/codemirror.bundle.mjs';

const SIX = ['New', 'Open', 'Run', 'Find', 'Edit', 'Add'];
// A window's furniture — title, tag line, status — in lines of text, and
// the strip of a container's tabs in pixels: what the compositor counts
// besides the body (compositor.md, Sizing).
const FURNITURE = 4.5;
const STRIP = 30;
const mac = typeof navigator !== 'undefined' && /Mac|iP(hone|ad|od)/.test(navigator.platform);

// CodeMirror's own classes, reached through a view: the state, for the facet
// that lets a selection be many ranges — Find's (type.md, text/plain) —
// and the selection's, to make one.
let CM;
function cm() {
  if (CM) return CM;
  const v = new EditorView({});
  CM = { EditorState: v.state.constructor, EditorSelection: v.state.selection.constructor };
  v.destroy();
  return CM;
}

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
};
const basename = (p) => p.replace(/\/$/, '').split('/').pop() || '/';

// An element's children made `kids`, touching nothing that is already so:
// a node taken out and put back loses the focus, and the line being typed
// with it.
function put(parent, kids) {
  const now = parent.childNodes;
  if (now.length === kids.length && kids.every((k, i) => now[i] === k)) return;
  parent.replaceChildren(...kids);
}

// A window's verbs file: `<label>\t<action>` lines (type.md, The /type file
// syntax), with comments.
function verbs(text) {
  return (text ?? '')
    .split('\n')
    .filter((l) => l.trim() && !l.startsWith('#'))
    .map((l) => {
      const [label, action = ''] = l.split('\t');
      return { label: label.trim(), action: action.trim() };
    });
}

export class Surface {
  constructor(root, emca, sys) {
    this.page = root;
    this.emca = emca;
    this.sys = sys;
    this.views = new Map();
    this.queued = false;
    emca.changed = () => this.draw();
    this.global = el('header', 'global');
    this.main = el('main', 'workspace');
    root.append(this.global, this.main);
    new ResizeObserver(() => this.draw()).observe(this.main);
    document.addEventListener('keydown', (e) => this.key(e));
    this.measure();
    this.draw();
  }

  // The text cell, in the page's own units: the compositor's measure
  // (compositor.md, The unit), so the breakpoints follow the text size.
  measure() {
    const p = el('span', 'mono', '0'.repeat(100));
    p.style.cssText = 'position:absolute;visibility:hidden;white-space:pre';
    document.body.append(p);
    const r = p.getBoundingClientRect();
    this.cell = { w: r.width / 100 || 8, h: r.height || 18 };
    p.remove();
  }

  draw() {
    if (this.queued) return;
    this.queued = true;
    requestAnimationFrame(() => {
      this.queued = false;
      this.render();
    });
  }

  render() {
    const e = this.emca;
    this.renderglobal();
    const seen = new Set();
    const cs = getComputedStyle(this.main);
    const width = this.main.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    const height = this.main.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
    const node = this.layout(e.root, width, height, seen);
    if (this.main.firstChild !== node) this.main.replaceChildren(node);
    for (const [id, v] of this.views) {
      if (!seen.has(id)) {
        v.destroy?.();
        this.views.delete(id);
      }
    }
  }

  // ---- the outermost window: the page's toolbar ---------------------------

  renderglobal() {
    const r = this.emca.root;
    const key = r.verbs;
    if (this.global.dataset.verbs === key) return;
    this.global.dataset.verbs = key;
    this.global.replaceChildren(el('span', 'brand', 'Saranos'));
    const bar = el('nav', 'toolbar');
    for (const v of verbs(r.verbs)) bar.append(this.button(v.label, () => this.verb(r, v)));
    this.global.append(bar);
  }

  // ---- the compositor ------------------------------------------------------

  // Lay `w` out in a `width` × `height` rectangle and answer its element.
  // A container gives rectangles to the children that fit and makes the
  // rest tabs in its strip (compositor.md, Sizing; emca.md, Allocation).
  layout(w, width, height, seen) {
    seen.add(w.id);
    if (w.leaf) return this.leaf(w).el;
    const v = this.container(w);
    const horiz = w.kind === 'row';
    const strip = [];
    let shown;
    if (w.kind === 'tabs') {
      shown = w.current && !w.current.gone ? [w.current] : w.children.slice(0, 1);
      strip.push(...w.children);
    } else {
      const visible = w.children.filter((c) => !c.hidden);
      const along = horiz ? width : height;
      // the minimums that fit, the one chosen last first; the excess
      // become tabs, from the end
      const first = visible.includes(w.current) ? w.current : null;
      const order = first ? [first, ...visible.filter((c) => c !== first)] : visible;
      shown = [];
      let used = 0;
      const cap = w === this.emca.root ? this.columns(width) : Infinity;
      for (const c of order) {
        const m = this.min(c, horiz);
        if (shown.length && (used + m > along || shown.length >= cap)) break;
        shown.push(c);
        used += m;
      }
      shown.sort((a, b) => visible.indexOf(a) - visible.indexOf(b));
      for (const c of w.children) if (!shown.includes(c)) strip.push(c);
    }
    v.strip.hidden = strip.length === 0;
    const key = strip.map((c) => `${c.id}:${c.title}:${shown.includes(c)}:${c === this.emca.focus}`).join('|');
    if (v.strip.dataset.key !== key) {
      v.strip.dataset.key = key;
      v.strip.replaceChildren(...strip.map((c) => this.tab(w, c, shown.includes(c))));
    }
    // the rectangles: minimum first, the rest by slack, then equally — and
    // each may give up its share of the gaps between them, which the
    // allocation does not count
    const strips = v.strip.hidden ? 0 : STRIP;
    const sizes = this.allocate(shown, horiz, (horiz ? width : height - strips));
    const kids = shown.map((c, i) => {
      const cw = horiz ? sizes[i] : width;
      const ch = horiz ? height - strips : w.kind === 'tabs' ? height - strips : sizes[i];
      const n = this.layout(c, cw, ch, seen);
      n.style.flex = w.kind === 'tabs' ? '1 1 auto' : `0 1 ${Math.max(0, Math.floor(sizes[i]))}px`;
      return n;
    });
    v.kids.className = `kids ${w.kind === 'row' ? 'across' : 'down'}`;
    put(v.kids, kids);
    return v.el;
  }

  // How many columns the root divides into at this width:
  // /type/inode/system/breakpoints, in characters (type.md).
  columns(width) {
    const cols = width / this.cell.w;
    if (cols >= 216) return 3;
    if (cols >= 144) return 2;
    return 1;
  }

  // A window's minimum and natural along an axis, in pixels (compositor.md,
  // Sizing: "every window declares two numbers along its parent's axis").
  // The minimum is the responsive rules': a leaf needs 72 columns, and a
  // body ten lines and its furniture (compositor.md, Responsive rules).
  min(w, horiz) {
    const { w: cw, h: lh } = this.cell;
    if (w.leaf) return horiz ? 72 * cw : lh * (10 + FURNITURE);
    return this.over(w, horiz, (c) => this.min(c, horiz));
  }

  // The natural: what the content wants — its lines for text, its entries
  // for a listing, a terminal's 24 lines for a shell — and across, its
  // longest line, from 72 to 100 columns.
  natural(w, horiz) {
    const { w: cw, h: lh } = this.cell;
    if (!w.leaf) return this.over(w, horiz, (c) => this.natural(c, horiz));
    const lines = w.body.split('\n');
    if (horiz) return Math.min(100, Math.max(72, ...lines.map((l) => l.length + 4))) * cw;
    return lh * (FURNITURE + (w.role === 'shell' ? 24 : Math.min(40, lines.length + 1)));
  }

  // A container's, from its children's: summed along its own axis, the
  // largest across it; a strip of tabs, the one showing, and the strip.
  over(w, horiz, f) {
    const tabs = w.kind === 'tabs';
    const kids = tabs ? [w.current ?? w.children[0]].filter(Boolean) : w.children.filter((c) => !c.hidden);
    const ns = kids.map(f);
    const along = !tabs && (w.kind === 'row') === horiz;
    return (along ? ns.reduce((a, b) => a + b, 0) : Math.max(0, ...ns)) + (tabs && !horiz ? STRIP : 0);
  }

  // 1. every allocated child its minimum; 2. the remainder in proportion to
  // natural less minimum, capped at natural; 3. what is left, equally.
  allocate(kids, horiz, total) {
    const mins = kids.map((c) => this.min(c, horiz));
    const nats = kids.map((c, i) => Math.max(mins[i], this.natural(c, horiz)));
    const sizes = [...mins];
    let left = total - mins.reduce((a, b) => a + b, 0);
    const slack = nats.map((n, i) => n - mins[i]);
    const all = slack.reduce((a, b) => a + b, 0);
    if (left > 0 && all > 0) {
      const give = Math.min(left, all);
      for (let i = 0; i < kids.length; i++) sizes[i] += (give * slack[i]) / all;
      left -= give;
    }
    if (left > 0 && kids.length) for (let i = 0; i < kids.length; i++) sizes[i] += left / kids.length;
    return sizes;
  }

  // ---- containers: rows, columns, tabs --------------------------------------

  container(w) {
    let v = this.views.get(w.id);
    if (!v || v.kind !== 'container') {
      v?.destroy?.();
      v = { kind: 'container', el: el('section', 'container'), strip: el('nav', 'strip'), kids: el('div', 'kids') };
      v.el.append(v.strip, v.kids);
      this.views.set(w.id, v);
    }
    v.el.dataset.kind = w.kind;
    v.el.dataset.id = w.id;
    return v;
  }

  // A child in a container's strip: a tab, which is a whole window without
  // a rectangle (emca.md, A TAB IS NOT A REDUCED WINDOW).
  tab(parent, c, shown) {
    const t = el('span', `tab${shown ? ' current' : ''}${c === this.emca.focus ? ' focused' : ''}`);
    const name = el('button', 'name', c.leaf ? basename(c.title) || c.title : `${c.kind} (${c.children.length})`);
    name.title = c.title || c.kind;
    name.addEventListener('click', () => this.emca.select(c));
    const x = el('button', 'x', '×');
    x.title = 'Close';
    x.addEventListener('click', () => this.emca.close(c));
    t.append(name, x);
    return t;
  }

  // ---- a window that holds content -------------------------------------------

  leaf(w) {
    let v = this.views.get(w.id);
    const kind = w.role === 'shell' ? 'shell' : w.type === 'inode/directory' ? 'listing' : 'text';
    if (!v || v.kind !== kind) {
      v?.destroy?.();
      v = this.makeleaf(w, kind);
      this.views.set(w.id, v);
    }
    this.update(w, v);
    return v;
  }

  makeleaf(w, kind) {
    const v = { kind, el: el('article', `window ${kind}`) };
    v.el.dataset.id = w.id;
    v.el.addEventListener('pointerdown', () => {
      if (this.emca.focus !== w) {
        this.emca.focus = w;
        this.draw();
      }
    });
    // title and window controls
    const bar = el('div', 'titlebar');
    const controls = el('span', 'controls');
    const ctl = (label, title, f) => {
      const b = this.button(label, f);
      b.title = title;
      b.className = 'control';
      controls.append(b);
    };
    ctl('×', 'Close', () => this.emca.close(this.emca.windows.get(w.id) ?? w));
    ctl('–', 'Minimise', () => this.emca.minimise(w));
    ctl('+', 'Maximise', () => this.emca.maximise(w));
    ctl('⫴', 'Duplicate as column', () => this.emca.duplicate(w, 'column'));
    ctl('☰', 'Duplicate as row', () => this.emca.duplicate(w, 'row'));
    ctl('⧉', 'Duplicate as tab', () => this.emca.duplicate(w, 'tab'));
    v.title = el('input', 'title');
    v.title.spellcheck = false;
    v.title.setAttribute('aria-label', 'title');
    v.title.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        this.emca.retitle(w, v.title.value.trim());
        v.title.blur();
      }
      if (e.key === 'Escape') {
        v.title.value = w.title;
        v.title.blur();
      }
    });
    v.title.addEventListener('blur', () => (v.title.value = w.title));
    v.role = el('span', 'role');
    bar.append(controls, v.title, v.role);
    // the tag line and its six, the separator, the toolbar
    const row = el('div', 'tagrow');
    v.tag = el('input', 'tag');
    v.tag.spellcheck = false;
    v.tag.placeholder = '';
    v.tag.setAttribute('aria-label', 'tag line');
    v.tag.addEventListener('input', () => (w.tag = v.tag.value));
    v.tag.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this.six(w, v, e.shiftKey ? 'Open' : 'Run');
    });
    const six = el('span', 'six');
    for (const s of SIX) six.append(this.button(s, () => this.six(w, v, s)));
    v.toolbar = el('span', 'toolbar');
    row.append(v.tag, six, el('span', 'sep'), v.toolbar);
    // the content
    v.body = el('div', 'body');
    if (kind === 'text') this.makeeditor(w, v);
    if (kind === 'listing') v.list = v.body.appendChild(el('ul', 'listing'));
    if (kind === 'shell') this.makeshell(w, v);
    // the status line
    v.status = el('div', 'status');
    v.el.append(bar, row, v.body, v.status);
    v.destroy = () => v.editor?.destroy();
    return v;
  }

  update(w, v) {
    const e = this.emca;
    v.el.classList.toggle('focused', e.focus === w);
    if (document.activeElement !== v.title) v.title.value = w.title;
    v.role.textContent = w.role && w.role !== 'manage' ? w.role : '';
    if (document.activeElement !== v.tag && v.tag.value !== w.tag) v.tag.value = w.tag;
    v.tag.classList.toggle('populated', !!w.tag.trim());
    // the toolbar: the type's verbs this role serves — Save only while there
    // is unsaved work, its appearance the dirty indicator (emca.md); Undo
    // and Redo only where a buffer is edited
    const vs = verbs(w.verbs).filter((x) => {
      if (x.action === 'host:save') return w.dirty;
      if (x.action === 'host:undo' || x.action === 'host:redo') return w.role === 'edit';
      return true;
    });
    for (const a of w.added ?? []) vs.push({ label: a, action: `run:${a}` });
    const key = vs.map((x) => x.label).join('|');
    if (v.toolbar.dataset.key !== key) {
      v.toolbar.dataset.key = key;
      v.toolbar.replaceChildren(...vs.map((x) => this.button(x.label, () => this.verb(w, x, v))));
    }
    const pos = v.editor ? this.position(v.editor) : '';
    v.status.textContent = [w.status.trim().replace(/\n/g, '   '), pos].filter(Boolean).join('   ');
    if (v.kind === 'text') this.updateeditor(w, v);
    if (v.kind === 'listing') this.updatelisting(w, v);
    if (v.kind === 'shell') this.updateshell(w, v);
  }

  // ---- text: look and edit, CodeMirror ----------------------------------------

  makeeditor(w, v) {
    const { EditorState } = cm();
    v.editable = w.role === 'edit';
    v.editor = new EditorView({
      parent: v.body,
      doc: w.body,
      extensions: [
        minimalSetup,
        EditorState.allowMultipleSelections.of(true),
        EditorView.editable.of(v.editable),
        EditorView.lineWrapping,
        EditorView.updateListener.of((u) => {
          // every edit, into emca's buffer: the two are mirrors (type.md,
          // The host half)
          if (u.docChanged) {
            w.body = u.state.doc.toString();
            v.syncing = true;
            this.emca.changed();
            v.syncing = false;
          }
          if (u.selectionSet) v.status.textContent = [w.status.trim(), this.position(v.editor)].filter(Boolean).join('   ');
        }),
      ],
    });
  }

  updateeditor(w, v) {
    if (v.editable !== (w.role === 'edit')) {
      v.editor.destroy();
      v.body.replaceChildren();
      this.makeeditor(w, v);
      return;
    }
    const doc = v.editor.state.doc.toString();
    if (doc !== w.body) v.editor.dispatch({ changes: { from: 0, to: doc.length, insert: w.body } });
  }

  position(view) {
    const h = view.state.selection.main.head;
    const line = view.state.doc.lineAt(h);
    return `at ${line.number}:${h - line.from + 1}`;
  }

  // ---- a listing: one name per line, each one tap from open ----------------
  //
  // The names are text, which a sweep selects as it would any other — any
  // text can be a verb's operand, listings alike (emca.md, The acceptance
  // test) — and a tap that selects nothing opens one.

  updatelisting(w, v) {
    if (v.shown === w.body) return;
    v.shown = w.body;
    v.list.replaceChildren(
      ...w.body
        .split('\n')
        .filter(Boolean)
        .map((name) => {
          const li = el('li');
          const n = el('span', 'name', name);
          n.tabIndex = 0;
          n.setAttribute('role', 'link');
          n.addEventListener('click', () => {
            if (!getSelection()?.toString()) this.emca.open(w, name);
          });
          n.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') this.emca.open(w, name);
          });
          li.append(n);
          return li;
        }),
    );
  }

  // ---- the shell role: the transcript, and the line held until Enter ---------

  makeshell(w, v) {
    v.log = el('pre', 'log');
    v.out = v.log.appendChild(document.createTextNode(''));
    v.line = el('span', 'line');
    v.line.contentEditable = 'plaintext-only';
    v.line.spellcheck = false;
    v.line.setAttribute('aria-label', 'what you type');
    v.log.append(v.line);
    v.body.append(v.log);
    v.body.addEventListener('click', () => {
      if (!getSelection().toString()) v.line.focus();
    });
    v.line.addEventListener('keydown', (e) => {
      const m = this.emca.windows.get(w.id);
      if (!m) return;
      if (e.ctrlKey && !e.metaKey && (e.key === 'c' || e.key === 'C')) {
        e.preventDefault();
        v.line.textContent = '';
        this.emca.interrupt(m);
        return;
      }
      if (m.raw) {
        e.preventDefault();
        const k = e.key === 'Enter' ? '\n' : e.key === 'Backspace' ? '\b' : e.key === 'Escape' ? '\x1b' : e.key === 'Tab' ? '\t' : e.key.length === 1 ? (e.ctrlKey ? String.fromCharCode(e.key.toUpperCase().charCodeAt(0) - 64) : e.key) : '';
        if (k) this.emca.type(m, k);
        return;
      }
      if (e.key === 'Enter') {
        e.preventDefault();
        const line = v.line.textContent;
        v.line.textContent = '';
        this.emca.type(m, line + '\n');
      } else if (e.ctrlKey && (e.key === 'd' || e.key === 'D')) {
        e.preventDefault();
        const line = v.line.textContent;
        v.line.textContent = '';
        this.emca.type(m, line);
        if (!line) this.emca.type(m, '');
      } else if (e.ctrlKey && (e.key === 'u' || e.key === 'U')) {
        e.preventDefault();
        v.line.textContent = '';
      }
    });
  }

  updateshell(w, v) {
    if (v.out.data === w.body) return;
    const atend = v.body.scrollTop + v.body.clientHeight >= v.body.scrollHeight - 4;
    v.out.data = w.body;
    if (atend) v.body.scrollTop = v.body.scrollHeight;
  }

  // ---- the verbs ---------------------------------------------------------------

  selection(v) {
    if (v.editor) {
      const s = v.editor.state;
      return s.sliceDoc(s.selection.main.from, s.selection.main.to);
    }
    return getSelection()?.toString() ?? '';
  }

  // The tag line's six: their operand is the tag line's text, or the
  // selection when the tag line is empty (emca.md, The tag line's six verbs).
  six(w, v, verb) {
    const e = this.emca;
    const sel = this.selection(v);
    const text = e.operand(w, sel);
    switch (verb) {
      case 'New': return e.newthing(w);
      case 'Open': return e.open(w, text);
      case 'Run': return e.run(w, w.tag.trim() || sel.trim(), sel);
      case 'Find': {
        const ranges = e.find(w, text);
        if (v.editor && ranges.length) {
          const { EditorSelection } = cm();
          v.editor.dispatch({
            selection: EditorSelection.create(ranges.map(([a, b]) => EditorSelection.range(a, b))),
            scrollIntoView: true,
          });
          v.editor.focus();
        }
        return ranges;
      }
      case 'Edit': return e.edit(w, w.tag.trim());
      case 'Add': return e.add(w, w.tag.trim());
    }
  }

  // A toolbar verb: `host:` asks the surface, `run:` runs a command, and
  // `manager:` asks the manager (type.md, Verb bindings take three forms).
  verb(w, x, v) {
    const e = this.emca;
    const [kind, what] = [x.action.slice(0, x.action.indexOf(':')), x.action.slice(x.action.indexOf(':') + 1)];
    if (kind === 'run') return e.run(w === e.root ? (e.focus ?? e.root) : w, what);
    if (kind !== 'host') return;
    switch (what) {
      case 'save': return e.save(w);
      case 'saveall': return e.saveall();
      case 'revert': return e.revert(w);
      case 'undo': case 'redo': return this.history(v, what);
      case 'interrupt': return e.interrupt(w);
      case 'reset': return e.reset();
      case 'newshell': return e.newshell();
      case 'reboot': return location.reload();
    }
  }

  // Keystroke undo is the surface's (compositor.md, Editing is the
  // surface's): the editor's own history, by its own keys.
  history(v, what) {
    if (!v?.editor) return;
    const redo = what === 'redo';
    v.editor.focus();
    v.editor.contentDOM.dispatchEvent(new KeyboardEvent('keydown', { key: redo && !mac ? 'y' : 'z', ctrlKey: !mac, metaKey: mac, shiftKey: redo && mac, bubbles: true, cancelable: true }));
  }

  // The keyboard grammar's first few (compositor.md): save, run, open.
  key(e) {
    const mod = mac ? e.metaKey : e.ctrlKey;
    const w = this.emca.focus;
    const v = w && this.views.get(w.id);
    if (!mod || !w || !v) return;
    if (e.key === 's') {
      e.preventDefault();
      this.emca.save(w);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      this.six(w, v, e.shiftKey ? 'Open' : 'Run');
    }
  }

  button(label, f) {
    const b = el('button', 'verb', label);
    b.type = 'button';
    b.addEventListener('click', (e) => {
      e.preventDefault();
      f();
    });
    return b;
  }
}
