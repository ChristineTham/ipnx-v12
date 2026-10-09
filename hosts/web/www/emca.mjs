// emca — the window manager, which is the host's (docs/emca.md; Christine,
// 2026-10-08: "emca is not rio, it is a full windowing manager implemented
// on host side"). This file is the model and the file server: which windows
// exist, how they nest, what each holds — and the files IPNX reaches them
// by (docs/window.md), served over `#9/1` as the store is served over
// `#9/0`. What a person sees and does is surface.mjs's; nothing here touches
// a page, so Node can test it.
//
// The division with IPNX (emca.md, "The split"): this side owns the tree,
// the composition and the drawing; IPNX owns what things ARE and what verbs
// MEAN. So when a window needs IPNX — what a path is, its content, a
// command run, a program started — emca writes an rc command line to the
// root window's `events` (window.md), and emca's IPNX half, an rc reading
// it (`/bin/emca`), runs it. `emcaopen` (docs/type.md) decides what a path is
// and fills the window.

import { T, QTDIR, DMDIR, OTRUNC, Reader, Writer, dir, error } from './ninep.mjs';

const enc = new TextEncoder();
const dec = new TextDecoder();

// rio's error for a window that has gone (rio/dat.h, Edeleted)
const EDELETED = 'window deleted';
const ENONEXIST = 'file does not exist';
const EPERM = 'permission denied';

// A word as rc reads it: bare if nothing in it is special, otherwise in
// single quotes with each quote doubled (rc(1), Quoting).
export function rcquote(w) {
  if (w !== '' && /^[A-Za-z0-9_\-./+,:=@%]+$/.test(w)) return w;
  return `'${w.replace(/'/g, "''")}'`;
}

export const dirname = (p) => (p === '/' ? '/' : p.replace(/\/[^/]*\/?$/, '') || '/');

const concat = (a, b) => {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
};

// Bytes spliced into bytes at `off`.
function splice(b, off, data) {
  const out = new Uint8Array(Math.max(b.length, off + data.length));
  out.set(b);
  out.set(data, off);
  return out;
}

// ---- the window ----------------------------------------------------------

// "A WINDOW IS A RECTANGLE WITH A TAG, CONTAINING EITHER A BODY OR CHILD
// WINDOWS" (emca.md, The window). A container's `kind` is the layout's word
// for how it divides — a `row`'s children side by side, a `column`'s top to
// bottom, `tabs` sharing one rectangle (type.md, The layout file).
export class Window {
  constructor(id) {
    this.id = id;
    this.title = '';      // a path: the title retargets (emca.md, TITLE)
    this.type = '';       // what the content is, a MIME type (type.md)
    this.role = '';       // which manager: look, edit, shell, manage
    this.kind = null;     // a container's row, column or tabs; null for a leaf
    this.parent = null;
    this.children = [];
    this.current = null;  // the child a tabs container shows
    this.hidden = false;  // minimised: out of its parent's allocation, a tab
    this.body = '';       // the content — always a file (window.md)
    this.saved = '';      // what was last read or written: dirty is the difference
    this.tag = '';        // the tag line: the operand (emca.md, TAG LINE)
    this.status = '';     // `<key> <value>` lines (type.md, The status line)
    this.verbs = '';      // the toolbar: `<label>\t<action>` lines (type.md, The /type file syntax)
    this.dir = '/';       // context: where Run runs and names resolve
    this.pid = 0;         // whose note group the interrupt goes to (wctl set -pid)
    this.input = [];      // committed input, waiting for a console read
    this.reads = [];      // console reads waiting for input
    this.raw = false;     // consctl rawon: every key crosses
    this.events = new Uint8Array(); // what whoever reads `events` has still to read
    this.eventreads = []; // reads of `events` waiting for something to read
    this.found = [];      // what the last Find selected, [start, end) each
    this.gone = false;
  }

  // Unsaved work: only where a buffer is edited (emca.md, Undo).
  get dirty() {
    return this.role === 'edit' && this.type !== 'inode/directory' && this.body !== this.saved;
  }

  get leaf() {
    return this.kind === null;
  }
}

// ---- the files -------------------------------------------------------------

// A window's own files (window.md, The per-window directory), at
// /dev/window/ in its namespace and at wsys/<n>/ for everyone; and acme's
// `ctl` (acme(4)), which `new/ctl` is.
const FILES = ['body', 'ctl', 'dirty', 'events', 'rect', 'role', 'status', 'tag', 'title', 'type', 'verbs', 'wctl'];
// What a filter's command reads and writes: the selected items, and what
// replaces them.
const FILTER = ['selection', 'replace'];
// What rio binds over /dev in a window (rio/fsys.c:25), of which these are
// this window manager's — the console is the shell role's.
const VIEW = ['cons', 'consctl', 'label', 'wctl', 'wdir', 'winid', 'winname'];
// An output's files (emca.md, Where command output goes): /proc's shape —
// what was fed in, what it wrote to 1 and to 2, captured separately, what
// it was, where it ran, how it ended, and the log, the session as it read.
const OUT = ['0', '1', '2', 'cmd', 'dir', 'log', 'status'];

// Qid paths: the root's own, then a window's or an output's in the high bits.
const QROOT = 1, QWSYS = 2, QNEW = 3, QLAYOUT = 4, QOUTPUT = 5, QNEWCTL = 6;
const WVIEW = 0x100, WFILES = 0x200, WOUT = 0x300;
const qpath = (n, k) => n * 0x10000 + k;

// ---- emca ----------------------------------------------------------------

export class Emca {
  // `reply(r)`: an R-message for the kernel. `changed()`: the model moved,
  // and the surface should draw it again.
  constructor({ reply = () => {}, changed = () => {} } = {}) {
    this.reply = reply;
    this.changed = changed;
    this.windows = new Map();
    this.next = 1;
    this.fids = new Map();
    this.focus = null;
    this.outputs = new Map(); // n → { cmd, dir, status, stdin, win }
    this.nextout = 1;
    this.layoutText = '';
    this.msize = 8192;
    // the root window, `/`, inode/system — the surface contains it
    // (emca.md, the agreed design, 2). The root is a row.
    this.root = this.make({ title: '/', type: 'inode/system', role: 'manage', kind: 'row' });
  }

  make(props = {}) {
    const w = new Window(this.next++);
    Object.assign(w, props);
    this.windows.set(w.id, w);
    return w;
  }

  // Put `w` under `parent`, at `index`, or at the end.
  adopt(parent, w, index = parent.children.length) {
    if (w.parent) this.orphan(w);
    w.parent = parent;
    parent.children.splice(index, 0, w);
    if (parent.kind === 'tabs' && !parent.current) parent.current = w;
  }

  orphan(w) {
    const p = w.parent;
    if (!p) return;
    const i = p.children.indexOf(w);
    p.children.splice(i, 1);
    if (p.current === w) p.current = p.children[Math.min(i, p.children.length - 1)] ?? null;
    w.parent = null;
  }

  firstleaf(w) {
    if (!w || w.gone) return null;
    if (w.leaf && w !== this.root) return w;
    const order = w.kind === 'tabs' && w.current ? [w.current, ...w.children] : w.children;
    for (const c of order) {
      const l = this.firstleaf(c);
      if (l) return l;
    }
    return null;
  }

  // ---- what IPNX is asked to do: the root window's `events` ------------

  // Run an rc command line in emca's IPNX half.
  ask(line) {
    this.post(this.root, line + '\n');
  }

  // A line for whoever reads `w`'s `events` (window.md: input the surface
  // does not consume, for the window's manager).
  post(w, line) {
    w.events = concat(w.events, enc.encode(line));
    this.drain(w);
  }

  // Ask IPNX what `path` is, and fill `w` with it: `emcaopen -w`.
  fill(w, path, role) {
    this.ask(`cd ${rcquote(w.dir)}; emcaopen -w ${w.id} ${rcquote(path)}${role ? ' ' + rcquote(role) : ''} >/dev/null`);
  }

  // ---- the layout ----------------------------------------------------------

  // `/type/inode/system/layout` (type.md, The layout file): each line a
  // path, `row` and `column` split, `tabs` groups, indentation nests, and a
  // path may be followed by the role it is opened in. Each path's window is
  // filled by IPNX, which says what it is.
  layout(text) {
    this.layoutText = text;
    for (const c of [...this.root.children]) this.close(c, false);
    const stack = [{ depth: -1, win: this.root }];
    for (const raw of text.split('\n')) {
      const line = raw.replace(/#.*/, '');
      if (!line.trim()) continue;
      const depth = line.match(/^\s*/)[0].replace(/\t/g, '    ').length;
      while (stack.length > 1 && stack[stack.length - 1].depth >= depth) stack.pop();
      const parent = stack[stack.length - 1].win;
      const [word, role] = line.trim().split(/\s+/);
      if (word === 'row' || word === 'column' || word === 'tabs') {
        const c = this.make({ kind: word, dir: parent.dir });
        this.adopt(parent, c);
        stack.push({ depth, win: c });
        continue;
      }
      const w = this.make({ title: word, dir: dirname(word) });
      this.adopt(parent, w);
      this.fill(w, word, role);
    }
    this.focus = this.firstleaf(this.root);
    this.changed();
  }

  // ---- the window operations (emca.md; window.md, The controls) ----------

  // Close: the window, what it holds, and the command running in it —
  // rio's "hangup" to its note group (rio/wind.c, Deleted).
  close(w, draw = true) {
    if (w === this.root || w.gone) return;
    for (const c of [...w.children]) this.close(c, false);
    // (a program that has finished has no note group left to hang up)
    if (w.pid) this.ask(`echo hangup >[2]/dev/null >/proc/${w.pid}/notepg`);
    w.gone = true;
    for (const r of w.reads.splice(0)) this.reply(error(r.tag, EDELETED));
    for (const r of w.eventreads.splice(0)) this.reply(error(r.tag, EDELETED));
    const p = w.parent;
    this.orphan(w);
    this.windows.delete(w.id);
    // a container with nothing left in it goes too
    if (p && p !== this.root && p.children.length === 0) this.close(p, false);
    if (!this.focus || this.focus.gone || this.focus === w) this.focus = this.firstleaf(this.root);
    if (draw) this.changed();
  }

  // Minimise: out of the parent's allocation — into its strip, a tab.
  // Maximise: every sibling out. Each undone by moving back (emca.md,
  // Allocation, and what a tab is).
  minimise(w) {
    w.hidden = !w.hidden;
    if (!w.hidden) this.select(w);
    this.changed();
  }

  maximise(w) {
    const p = w.parent;
    if (!p) return;
    const others = p.children.filter((c) => c !== w);
    const all = others.length > 0 && others.every((c) => c.hidden);
    for (const c of others) c.hidden = !all;
    w.hidden = false;
    this.select(w);
  }

  // Show `w`: the current tab wherever it is one, back in its parent's
  // allocation, and in focus.
  select(w) {
    // every container on the way up shows it: a tab's, and a row's or a
    // column's that had no room for it — it is one click away
    // (compositor.md, Responsive rules)
    for (let c = w; c.parent; c = c.parent) {
      c.parent.current = c;
      if (c === w) c.hidden = false;
    }
    const l = w.leaf ? w : this.firstleaf(w);
    if (l) this.focus = l;
    this.changed();
  }

  // Duplicate — as column, as row, as tab: the window again, a new window
  // on the same title, beside it, below it, or as a tab (compositor.md:
  // "duplicate renders as three buttons here"). Only a person splits.
  duplicate(w, how) {
    if (!w.leaf) return null;
    const copy = this.make({ title: w.title, dir: w.dir });
    if (how === 'tab') this.astab(w, copy);
    else this.split(w, copy, how === 'column' ? 'row' : 'column');
    this.fill(copy, w.title, w.role === 'shell' ? 'manage' : w.role);
    this.select(copy);
    return copy;
  }

  // `copy` beside `w` along `axis`: in `w`'s container if it runs that way;
  // otherwise `w` gains a neighbour in a new container where it stood —
  // alternation (compositor.md, Rows and columns: "the column will always
  // be on a child window").
  split(w, copy, axis) {
    let at = w;
    if (at.parent?.kind === 'tabs') at = at.parent;
    const p = at.parent ?? this.root;
    if (p.kind === axis) {
      this.adopt(p, copy, p.children.indexOf(at) + 1);
      return;
    }
    const c = this.make({ kind: axis, dir: at.dir });
    const i = p.children.indexOf(at);
    this.orphan(at);
    this.adopt(p, c, i);
    if (p.kind === 'tabs') p.current = c;
    this.adopt(c, at);
    this.adopt(c, copy);
  }

  // `w` as a tab beside `near` — "an open window opens a new tab by
  // default" (Christine, 2026-09-02): in `near`'s tab group, or in one made
  // where `near` stands.
  astab(near, w) {
    if (!near || near === this.root) {
      this.adopt(this.root, w);
      return;
    }
    let p = near.parent;
    if (p.kind !== 'tabs') {
      const g = this.make({ kind: 'tabs', dir: near.dir });
      const i = p.children.indexOf(near);
      this.orphan(near);
      this.adopt(p, g, i);
      if (p.kind === 'tabs') p.current = g;
      this.adopt(g, near);
      g.hidden = near.hidden;
      near.hidden = false;
      p = g;
    }
    this.adopt(p, w, p.children.indexOf(near) + 1);
    p.current = w;
  }

  // ---- the tag line's six (emca.md, The tag line's six verbs) -------------

  // The operand: the tag line's text, or — empty — the selection.
  operand(w, selection = '') {
    return w.tag.trim() || selection.trim();
  }

  // A name resolved against the window's directory.
  resolve(w, text) {
    const path = text.startsWith('/') ? text : `${w.dir.replace(/\/$/, '')}/${text}`;
    const parts = [];
    for (const p of path.split('/')) {
      if (p === '' || p === '.') continue;
      if (p === '..') parts.pop();
      else parts.push(p);
    }
    return '/' + parts.join('/');
  }

  // Open: a new window whose title is this text, reusing one already open
  // on it. A name ending in / is a directory, made now.
  open(w, text) {
    if (!text) return null;
    const path = this.resolve(w, text);
    for (const o of this.windows.values()) {
      if (o.leaf && o !== this.root && o.title === path && o.role !== 'shell') {
        this.select(o);
        return o;
      }
    }
    const n = this.make({ title: path, dir: dirname(path) });
    this.astab(w === this.root ? this.focus : w, n);
    if (text.endsWith('/')) this.ask(`mkdir -p ${rcquote(path)}`);
    this.fill(n, path);
    this.select(n);
    return n;
  }

  // Run: the text as a command, in the window's directory, ignoring dot —
  // its output into an output window, /output/<n>/log (emca.md, Where
  // command output goes). `|cmd`, `>cmd` and `<cmd` are acme's: the selected
  // items through, into or from a command.
  run(w, text, selection = '') {
    if (!text) return null;
    const c = text[0];
    if (c === '>') return this.output(w, text.slice(1).trim(), selection && !selection.endsWith('\n') ? selection + '\n' : selection);
    if (c === '|' || c === '<') return this.filter(w, c, text.slice(1).trim(), selection);
    return this.output(w, text, null);
  }

  output(w, cmd, stdin) {
    const n = this.nextout++;
    const out = this.make({ title: `/output/${n}/log`, type: 'text/plain', role: 'look', dir: w.dir });
    const dec = () => new TextDecoder();
    this.outputs.set(n, { cmd, dir: w.dir, status: '', stdin: stdin ?? '', out: '', err: '', win: out, dec: { 1: dec(), 2: dec(), log: dec() } });
    // the log is the session as it read: emca emits the cd line, so the
    // log is self-contained
    out.body = out.saved = `% cd ${w.dir}\n% ${cmd}\n`;
    out.status = `running ${cmd}`;
    this.astab(w === this.root ? this.focus : w, out);
    this.select(out);
    const o = `/mnt/wsys/output/${n}`;
    const input = stdin === null ? '/dev/null' : `${o}/0`;
    this.ask(`@{cd ${rcquote(w.dir)}; rc -c ${rcquote(cmd)} <${input} >>${o}/1 >>[2]${o}/2; echo $status >${o}/status} &`);
    return out;
  }

  // `|cmd` replaces the selection with the command's output, fed the
  // selection; `<cmd` replaces it with the output alone. With nothing
  // selected, the whole body is the selection.
  filter(w, how, cmd, selection) {
    w.selection = selection || w.body;
    const d = `/mnt/wsys/wsys/${w.id}`;
    const input = how === '|' ? `${d}/selection` : '/dev/null';
    this.ask(`@{cd ${rcquote(w.dir)}; rc -c ${rcquote(cmd)} <${input} >${d}/replace >[2=1]} &`);
    return w;
  }

  // Find: select every item the text names — `,x/text/`, so dot is the set
  // of all matches; a leading : or # is an address (emca.md, Find). The
  // status line shows the count (type.md, text/plain).
  find(w, text) {
    if (!text) return [];
    const body = w.body;
    let ranges = [];
    if (text.startsWith(':')) {
      const n = parseInt(text.slice(1), 10);
      const lines = body.split('\n');
      if (n >= 1 && n <= lines.length) {
        const start = lines.slice(0, n - 1).reduce((a, l) => a + l.length + 1, 0);
        ranges = [[start, start + lines[n - 1].length]];
      }
    } else if (text.startsWith('#')) {
      const n = Math.min(parseInt(text.slice(1), 10) || 0, body.length);
      ranges = [[n, n]];
    } else {
      for (let i = body.indexOf(text); i >= 0; i = body.indexOf(text, i + text.length)) ranges.push([i, i + text.length]);
    }
    w.found = ranges;
    w.status = `matches ${ranges.length}`;
    this.changed();
    return ranges;
  }

  // Edit: the text as a sam command over the window's text, whose result
  // replaces it. sam -d is sam without its terminal, reading commands from
  // its input (sam.c:45); it edits a file, so the text is one for the while.
  edit(w, cmd) {
    if (!cmd || w.role !== 'edit' || w.type === 'inode/directory') return;
    w.selection = '';
    const d = `/mnt/wsys/wsys/${w.id}`;
    const t = `/tmp/emca.${w.id}`;
    this.ask(`@{cat ${d}/body >${t}; {echo ${rcquote(cmd)}; echo w; echo q} | sam -d ${t} >/dev/null >[2=1]; cat ${t} >${d}/replace; rm -f ${t}} &`);
  }

  // Add: the tag line's text, a button on this window's toolbar — written
  // to its type's verbs, in the person's own /home/type, which binds over
  // the system's (emca.md, TOOLBAR).
  add(w, text) {
    if (!text || !w.type) return;
    w.verbs += `${text}\trun:${text}\n`;
    this.ask(`mkdir -p /usr/$user/type/${w.type} && echo ${rcquote(`${text}\trun:${text}`)} >>/usr/$user/type/${w.type}/verbs`);
    this.changed();
  }

  // New: the manager's (type.md). In a listing, a new file here — opened by
  // a name that does not exist yet, which Save makes.
  newthing(w) {
    if (w.type !== 'inode/directory' && w.type !== 'inode/system') return null;
    const names = new Set(w.body.split('\n'));
    let n = 1;
    while (names.has(`new${n}`)) n++;
    return this.open(w, `${w.title.replace(/\/$/, '')}/new${n}`);
  }

  // Save: the body back into its file, over 9P — an ordinary write
  // (emca.md, The channels).
  save(w) {
    if (!w.dirty) return;
    this.ask(`cat /mnt/wsys/wsys/${w.id}/body >${rcquote(w.title)} && echo clean >/mnt/wsys/wsys/${w.id}/ctl`);
  }

  saveall() {
    for (const w of this.windows.values()) if (w.dirty) this.save(w);
  }

  // Revert: read it again.
  revert(w) {
    if (w.role === 'shell' || w.title.startsWith('/output/')) return;
    this.fill(w, w.title, w.role);
  }

  // Interrupt: the shell role's — "interrupt" to the window's note group,
  // as rio writes a window's notepg for DEL (rio/wind.c:651); and what was
  // typed and not sent is dropped (:652). A reader of the console the note
  // finds waiting is the kernel's to end: its read is flushed (flush(5);
  // devmnt.c:782), and the flush drops it here.
  interrupt(w) {
    w.input = [];
    if (!w.pid) return;
    this.ask(`echo interrupt >[2]/dev/null >/proc/${w.pid}/notepg`);
  }

  // The title retargets: the window shows what the new title names, and
  // its type follows (emca.md, TITLE).
  retitle(w, title) {
    if (!title) return;
    const path = this.resolve(w, title);
    if (path === w.title) return;
    w.title = path;
    w.dir = dirname(path);
    w.body = w.saved = '';
    this.fill(w, path);
    this.changed();
  }

  // Reset: the root's own — the layout again (compositor.md, Fit and Reset).
  reset() {
    this.layout(this.layoutText);
  }

  // ---- the shell role: what is typed ----------------------------------------

  // Committed text — "only committed text crosses" (type.md, The shell
  // role) — for the console's reader. The surface held the line and showed
  // it as it was typed; committed, it joins the transcript, which is the
  // history (type.md, `shell`). In raw mode nothing is held, and nothing
  // shown but what the program writes.
  type(w, text) {
    if (!w.raw) {
      w.body = w.saved = w.body + text;
      this.changed();
    }
    w.input.push(enc.encode(text));
    this.feed(w);
  }

  // New Shell: inode/system's (type.md) — `manage` on /bin/rc, as a tab.
  newshell(near = this.focus) {
    const w = this.make({ title: '/bin/rc', dir: near?.dir ?? '/' });
    this.astab(near ?? this.root, w);
    this.fill(w, '/bin/rc', 'manage');
    this.select(w);
    return w;
  }

  feed(w) {
    while (w.reads.length && w.input.length) {
      const r = w.reads.shift();
      let b = w.input.shift();
      if (b.length > r.count) {
        w.input.unshift(b.subarray(r.count));
        b = b.subarray(0, r.count);
      }
      this.reply(new Writer().data(b).frame(T.read + 1, r.tag));
    }
  }

  // What waits in `events`, to readers as they ask: as many bytes as
  // each asks for — rc reads a buffer at a time, read(1) a byte.
  drain(w) {
    while (w.eventreads.length && w.events.length) {
      const r = w.eventreads.shift();
      const n = Math.min(r.count, w.events.length);
      this.reply(new Writer().data(w.events.slice(0, n)).frame(T.read + 1, r.tag));
      w.events = w.events.slice(n);
    }
  }

  // ---- the file server ----------------------------------------------------

  // One T-message. Its reply goes to `reply` — now, or for a read that
  // must wait, when there is something to answer.
  serve(t) {
    const m = new Reader(t);
    m.u32();
    const type = m.u8();
    const tag = m.u16();
    let r;
    try {
      r = this.t(type, tag, m);
    } catch (e) {
      r = error(tag, e.message || String(e));
    }
    if (r) this.reply(r);
  }

  t(type, tag, m) {
    switch (type) {
      case T.version: {
        const msize = m.u32();
        const v = m.s();
        this.msize = Math.min(msize, 64 * 1024);
        this.fids.clear();
        return new Writer().u32(this.msize).s(v.startsWith('9P2000') ? '9P2000' : 'unknown').frame(T.version + 1, tag);
      }
      case T.auth:
        return error(tag, 'authentication not required');
      case T.attach: {
        const fid = m.u32();
        m.u32();
        m.s();
        const aname = m.s();
        const node = aname === '' ? { k: 'root' } : this.windowview(aname);
        this.fids.set(fid, { node });
        return new Writer().qid(this.qid(node)).frame(T.attach + 1, tag);
      }
      case T.flush: {
        // flush(5): the request named is answered no more
        const old = m.u16();
        for (const w of this.windows.values()) {
          w.reads = w.reads.filter((r) => r.tag !== old);
          w.eventreads = w.eventreads.filter((r) => r.tag !== old);
        }
        return new Writer().frame(T.flush + 1, tag);
      }
      case T.walk: {
        const fid = m.u32(), newfid = m.u32(), n = m.u16();
        let node = this.fid(fid).node;
        const qids = [];
        for (let i = 0; i < n; i++) {
          const next = this.walk(node, m.s());
          if (!next) break;
          node = next;
          qids.push(this.qid(node));
        }
        if (n > 0 && qids.length === 0) return error(tag, ENONEXIST);
        if (qids.length === n) this.fids.set(newfid, { node });
        const w = new Writer().u16(qids.length);
        for (const q of qids) w.qid(q);
        return w.frame(T.walk + 1, tag);
      }
      case T.open: {
        const fid = m.u32(), mode = m.u8();
        const f = this.fid(fid);
        if (f.node.k === 'newctl') f.node = this.created();
        f.mode = mode;
        this.opened(f, mode);
        return new Writer().qid(this.qid(f.node)).u32(0).frame(T.open + 1, tag);
      }
      case T.create:
        return error(tag, EPERM);
      case T.read: {
        const fid = m.u32(), off = Number(m.u64()), count = m.u32();
        return this.read(this.fid(fid), tag, off, Math.min(count, this.msize - 24));
      }
      case T.write: {
        const fid = m.u32(), off = Number(m.u64()), count = m.u32();
        this.write(this.fid(fid), off, m.bytes(count));
        return new Writer().u32(count).frame(T.write + 1, tag);
      }
      case T.clunk: {
        const fid = m.u32();
        const f = this.fids.get(fid);
        this.fids.delete(fid);
        if (f) this.clunked(f);
        return new Writer().frame(T.clunk + 1, tag);
      }
      case T.remove:
        return error(tag, EPERM);
      case T.stat: {
        const d = this.stat(this.fid(m.u32()).node);
        return new Writer().u16(d.length).push(d).frame(T.stat + 1, tag);
      }
      case T.wstat:
        // what a truncating create of an existing file asks: accepted, as
        // nothing about a window is a file's metadata
        return new Writer().frame(T.wstat + 1, tag);
      default:
        return error(tag, 'bad 9P message');
    }
  }

  fid(n) {
    const f = this.fids.get(n);
    if (!f) throw new Error('unknown fid');
    return f;
  }

  // An attach naming a window: what that window's namespace binds over
  // /dev, as rio's attach names its window (rio/fsys.c, Tattach).
  windowview(aname) {
    const w = /^\d+$/.test(aname) && this.windows.get(Number(aname));
    if (!w) throw new Error(`no window ${aname}`);
    return { k: 'view', w: w.id };
  }

  // An open of new/ctl makes a window — acme's `new` (acme(4)): "accessing
  // any file in new creates a new window" — a tab beside the one in focus.
  created() {
    const w = this.make({ dir: this.focus?.dir ?? '/' });
    this.astab(this.focus ?? this.root, w);
    this.select(w);
    return { k: 'file', w: w.id, name: 'ctl' };
  }

  // What is written through a fid is gathered as bytes — a write may end
  // in the middle of a character — and takes effect when it is clunked.
  opened(f, mode) {
    const n = f.node;
    if (n.k === 'layout') f.buf = new Uint8Array();
    if (n.k !== 'file') return;
    const w = this.windows.get(n.w);
    if (!w) return;
    if (n.name === 'body') f.buf = mode & OTRUNC ? new Uint8Array() : enc.encode(w.body);
    if (n.name === 'replace') f.buf = new Uint8Array();
  }

  clunked(f) {
    const n = f.node;
    if (n.k === 'layout' && f.buf) {
      this.layout(dec.decode(f.buf));
      return;
    }
    if (n.k !== 'file' || !f.buf) return;
    const w = this.windows.get(n.w);
    if (!w) return;
    if (n.name === 'body') {
      // the content as written: what was last read, so clean
      w.body = w.saved = dec.decode(f.buf);
    }
    if (n.name === 'replace') {
      // a filter's output, or an Edit's: in place of what it was given
      const r = dec.decode(f.buf);
      const s = w.selection;
      if (s && s !== w.body && w.body.includes(s)) w.body = w.body.replace(s, r);
      else w.body = r;
      w.selection = undefined;
    }
    this.changed();
  }

  walk(node, name) {
    if (name === '..') return node.up ?? node;
    const at = (n) => ({ ...n, up: node });
    switch (node.k) {
      case 'root':
        if (name === 'wsys') return at({ k: 'wsys' });
        if (name === 'new') return at({ k: 'new' });
        if (name === 'layout') return at({ k: 'layout' });
        if (name === 'output') return at({ k: 'output' });
        return null;
      case 'wsys': {
        const w = /^\d+$/.test(name) && this.windows.get(Number(name));
        return w ? at({ k: 'files', w: w.id }) : null;
      }
      case 'new':
        return name === 'ctl' ? at({ k: 'newctl' }) : null;
      case 'view':
        if (name === 'window') return at({ k: 'files', w: node.w });
        if (name === 'wsys') return at({ k: 'wsys' });
        return VIEW.includes(name) ? at({ k: 'file', w: node.w, name }) : null;
      case 'files':
        return FILES.includes(name) || FILTER.includes(name) ? at({ k: 'file', w: node.w, name }) : null;
      case 'output': {
        const n = /^\d+$/.test(name) && this.outputs.has(Number(name)) ? Number(name) : 0;
        return n ? at({ k: 'outn', n }) : null;
      }
      case 'outn':
        return OUT.includes(name) ? at({ k: 'out', n: node.n, name }) : null;
      default:
        return null;
    }
  }

  qid(node) {
    const d = (path) => ({ type: QTDIR, path });
    const f = (path) => ({ type: 0, path });
    switch (node.k) {
      case 'root': return d(QROOT);
      case 'wsys': return d(QWSYS);
      case 'new': return d(QNEW);
      case 'newctl': return f(QNEWCTL);
      case 'layout': return f(QLAYOUT);
      case 'output': return d(QOUTPUT);
      case 'outn': return d(qpath(node.n, WOUT));
      case 'out': return f(qpath(node.n, WOUT + 1 + OUT.indexOf(node.name)));
      case 'view': return d(qpath(node.w, WVIEW));
      case 'files': return d(qpath(node.w, WFILES));
      case 'file': return f(qpath(node.w, 1 + [...VIEW, ...FILES, ...FILTER].indexOf(node.name)));
    }
    return f(0);
  }

  name(node) {
    switch (node.k) {
      case 'root': return '/';
      case 'view': case 'files': return `${node.w}`;
      case 'outn': return `${node.n}`;
      case 'file': case 'out': return node.name;
      case 'newctl': return 'ctl';
      default: return node.k;
    }
  }

  stat(node) {
    const q = this.qid(node);
    const isdir = q.type & QTDIR;
    const length = isdir ? 0 : this.contents(node).length;
    const mode = isdir ? DMDIR | 0o755 : node.k === 'out' && (node.name === 'cmd' || node.name === 'dir') ? 0o444 : 0o666;
    return dir({ qid: q, mode, length, name: this.name(node), mtime: Math.floor(Date.now() / 1000) });
  }

  // The entries of a directory.
  list(node) {
    const names = {
      root: ['layout', 'new', 'output', 'wsys'],
      wsys: [...this.windows.keys()].map(String),
      new: ['ctl'],
      view: [...VIEW, 'window', 'wsys'],
      files: FILES,
      output: [...this.outputs.keys()].map(String),
      outn: OUT,
    }[node.k] ?? [];
    return names.map((n) => this.walk(node, n)).filter(Boolean);
  }

  // What a file reads as.
  contents(node) {
    if (node.k === 'layout') return enc.encode(this.layoutText);
    if (node.k === 'out') {
      const o = this.outputs.get(node.n);
      if (!o) return new Uint8Array();
      return enc.encode({ 0: o.stdin, 1: o.out, 2: o.err, cmd: o.cmd + '\n', dir: o.dir + '\n', status: o.status, log: o.win.body }[node.name]);
    }
    if (node.k !== 'file') return new Uint8Array();
    const w = this.windows.get(node.w);
    if (!w) return new Uint8Array();
    switch (node.name) {
      case 'body': return enc.encode(w.body);
      case 'tag': return enc.encode(w.tag);
      case 'title': case 'winname': case 'label': return enc.encode(w.title);
      case 'type': return enc.encode(w.type);
      case 'role': return enc.encode(w.role);
      case 'status': return enc.encode(w.status);
      case 'verbs': return enc.encode(w.verbs);
      case 'dirty': return enc.encode(w.dirty ? '1' : '0');
      case 'winid': return enc.encode(`${String(w.id).padStart(11)} `);
      case 'wdir': return enc.encode(w.dir);
      case 'selection': return enc.encode(w.selection ?? '');
      // acme's ctl: the id, the tag's and body's lengths, isdir, isdirty
      case 'ctl': return enc.encode([w.id, w.tag.length, w.body.length, w.type === 'inode/directory' ? 1 : 0, w.dirty ? 1 : 0].map((x) => String(x).padStart(11)).join(' ') + ' ');
      case 'rect': {
        const r = w.rect ?? { x: 0, y: 0, w: 0, h: 0 };
        return enc.encode(`${r.x} ${r.y} ${r.w} ${r.h}\n`);
      }
      default: return new Uint8Array();
    }
  }

  read(f, tag, off, count) {
    const node = f.node;
    if (this.qid(node).type & QTDIR) {
      // a directory reads as whole entries, from where the last read ended
      if (off === 0) {
        f.entries = this.list(node).map((n) => this.stat(n));
        f.at = 0;
      }
      const out = [];
      let n = 0;
      while (f.at < f.entries.length && n + f.entries[f.at].length <= count) {
        out.push(f.entries[f.at]);
        n += f.entries[f.at++].length;
      }
      const b = new Uint8Array(n);
      let i = 0;
      for (const e of out) { b.set(e, i); i += e.length; }
      return new Writer().data(b).frame(T.read + 1, tag);
    }
    // the console and `events` wait for something to say
    if (node.k === 'file' && (node.name === 'cons' || node.name === 'events')) {
      const w = this.windows.get(node.w);
      if (!w) return error(tag, EDELETED);
      if (node.name === 'cons') {
        w.reads.push({ tag, count });
        this.feed(w);
      } else {
        w.eventreads.push({ tag, count });
        this.drain(w);
      }
      return null;
    }
    const b = this.contents(node);
    return new Writer().data(b.subarray(Math.min(off, b.length), Math.min(off + count, b.length))).frame(T.read + 1, tag);
  }

  write(f, off, data) {
    const node = f.node;
    if (node.k === 'layout') {
      f.buf = splice(f.buf ?? new Uint8Array(), off, data);
      return;
    }
    if (node.k === 'out') {
      const o = this.outputs.get(node.n);
      if (!o) return;
      if (node.name === '1' || node.name === '2' || node.name === 'log') {
        const text = o.dec[node.name].decode(data, { stream: true });
        if (node.name === '1') o.out += text;
        if (node.name === '2') o.err += text;
        // the log is both, as they came
        o.win.body = o.win.saved = o.win.body + text;
      }
      if (node.name === 'status') {
        o.status = dec.decode(data).trim();
        o.win.status = o.status ? `exited ${o.status}` : 'exited';
        // and how it ended, the last line of the conversation (emca.md)
        const log = o.win.body;
        o.win.body = o.win.saved = `${log}${log && !log.endsWith('\n') ? '\n' : ''}exit${o.status ? ' ' + o.status : ''}\n`;
      }
      if (node.name === '0') o.stdin = dec.decode(data);
      this.changed();
      return;
    }
    if (node.k !== 'file') throw new Error(EPERM);
    const w = this.windows.get(node.w);
    if (!w) throw new Error(EDELETED);
    const text = () => dec.decode(data);
    switch (node.name) {
      case 'cons':
        // what the window's programs write: the transcript grows
        w.consdec ??= new TextDecoder();
        w.body = w.saved = w.body + w.consdec.decode(data, { stream: true });
        break;
      case 'consctl':
        for (const word of text().split(/\s+/)) {
          if (word === 'rawon') w.raw = true;
          if (word === 'rawoff') w.raw = false;
        }
        break;
      case 'body':
      case 'replace':
        f.buf = splice(f.buf ?? enc.encode(node.name === 'body' ? w.body : ''), off, data);
        return;
      case 'tag':
        w.tag = text().replace(/\n$/, '');
        break;
      case 'title': case 'label':
        w.title = text().trim();
        w.dir = w.type === 'inode/directory' ? w.title : dirname(w.title);
        break;
      case 'type':
        w.type = text().trim();
        w.dir = w.type === 'inode/directory' || w.type === 'inode/system' ? w.title || w.dir : dirname(w.title || w.dir);
        break;
      case 'role':
        w.role = text().trim();
        break;
      case 'status':
        w.status = off === 0 ? text() : w.status + text();
        break;
      case 'verbs':
        w.verbs = off === 0 ? text() : w.verbs + text();
        break;
      case 'wdir':
        w.dir = text().trim();
        break;
      case 'ctl':
      case 'wctl':
        this.ctl(w, text());
        break;
      default:
        throw new Error(EPERM);
    }
    this.changed();
  }

  // `wctl` — rio's verbs where this window manager has them (rio/wctl.c:35:
  // `set -pid`, `delete`, `hide`, `unhide`, `current`) — and acme's `ctl`
  // messages (acme(4): `clean`, `dirty`, `del`, `name`).
  ctl(w, text) {
    for (const line of text.split('\n')) {
      const a = line.trim().split(/\s+/);
      switch (a[0]) {
        case 'set': {
          const i = a.indexOf('-pid');
          if (i >= 0) w.pid = Number(a[i + 1]) || 0;
          break;
        }
        case 'delete': case 'del': this.close(w); break;
        case 'hide': w.hidden = true; break;
        case 'unhide': w.hidden = false; break;
        case 'current': this.select(w); break;
        case 'clean': w.saved = w.body; break;
        case 'dirty': w.saved = null; break;
        case 'name': w.title = a.slice(1).join(' '); break;
      }
    }
  }
}
