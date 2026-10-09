// emca's model and file server, alone: no kernel, no page. The kernel's
// side of each exchange is played here as its mount driver would play it.
//
//   node --test hosts/web/test/emca.test.mjs

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Emca, rcquote } from '../www/emca.mjs';
import { T, Writer, Reader, OTRUNC } from '../www/ninep.mjs';

const enc = new TextEncoder();
const dec = new TextDecoder();

// A client: sends T-messages, keeps the replies by tag.
function client() {
  const replies = new Map();
  const e = new Emca({ reply: (r) => replies.set(new DataView(r.buffer, r.byteOffset).getUint16(5, true), r) });
  let tag = 1;
  const rpc = (type, build) => {
    const t = tag++;
    e.serve(build(new Writer()).frame(type, t));
    return t;
  };
  const answer = (t) => {
    const r = replies.get(t);
    if (!r) return null;
    replies.delete(t);
    const m = new Reader(r);
    m.u32();
    const type = m.u8();
    m.u16();
    if (type === T.error + 1) throw new Error(m.s());
    return m;
  };
  const now = (type, build) => answer(rpc(type, build));
  now(T.version, (w) => w.u32(8192).s('9P2000'));
  let fid = 100;
  const walk = (from, names) => {
    const f = fid++;
    now(T.walk, (w) => {
      w.u32(from).u32(f).u16(names.length);
      for (const n of names) w.s(n);
      return w;
    });
    return f;
  };
  const attach = (aname) => {
    const f = fid++;
    now(T.attach, (w) => w.u32(f).u32(0xffffffff).s('kitty').s(aname));
    return f;
  };
  const open = (f, mode = 0) => now(T.open, (w) => w.u32(f).u8(mode));
  const write = (f, s, off = 0) => now(T.write, (w) => w.u32(f).u64(off).data(enc.encode(s)));
  const clunk = (f) => now(T.clunk, (w) => w.u32(f));
  const read = (f, off = 0, n = 8192) => {
    const m = now(T.read, (w) => w.u32(f).u64(off).u32(n));
    return m && dec.decode(m.bytes(m.u32()));
  };
  // a file, written whole as `echo -n x >file` writes it
  const put = (root, path, s) => {
    const f = walk(root, path.split('/'));
    open(f, 1 | OTRUNC);
    write(f, s);
    clunk(f);
  };
  const get = (root, path) => {
    const f = walk(root, path.split('/'));
    open(f, 0);
    const s = read(f);
    clunk(f);
    return s;
  };
  return { e, rpc, answer, attach, walk, open, write, read, clunk, put, get };
}

// What emca asked of IPNX, a line each.
function asked(e) {
  const lines = dec.decode(e.root.events).split('\n').filter(Boolean);
  e.root.events = new Uint8Array();
  return lines;
}

test('rc reads the quoting emca writes', () => {
  assert.equal(rcquote('/etc/motd'), '/etc/motd');
  assert.equal(rcquote("it's here"), "'it''s here'");
  assert.equal(rcquote(''), "''");
});

test('the layout makes the tree, and asks IPNX what each path is', () => {
  const c = client();
  const root = c.attach('');
  c.put(root, 'layout', '/home\ncolumn\n\ttabs\n\t\t/etc/motd\n\t\t/bin/tour\n\t\t/home/README\n\t/bin/rc\n');
  const r = c.e.root;
  assert.equal(r.kind, 'row');
  assert.deepEqual(r.children.map((w) => w.kind ?? w.title), ['/home', 'column']);
  const col = r.children[1];
  assert.deepEqual(col.children.map((w) => w.kind ?? w.title), ['tabs', '/bin/rc']);
  assert.deepEqual(col.children[0].children.map((w) => w.title), ['/etc/motd', '/bin/tour', '/home/README']);
  assert.equal(col.children[0].current.title, '/etc/motd', 'the first tab shows');
  const lines = asked(c.e);
  assert.equal(lines.length, 5);
  assert.match(lines[0], /^cd \/; emcaopen -w \d+ \/home >\/dev\/null$/);
  assert.match(lines[4], /^cd \/bin; emcaopen -w \d+ \/bin\/rc >\/dev\/null$/);
  assert.equal(c.e.focus.title, '/home');
});

test('IPNX fills a window through its files, and the body read back is what was written', () => {
  const c = client();
  const root = c.attach('');
  const w = c.e.make({ title: '/etc/motd' });
  c.e.adopt(c.e.root, w);
  c.put(root, `wsys/${w.id}/type`, 'text/plain');
  c.put(root, `wsys/${w.id}/role`, 'edit');
  c.put(root, `wsys/${w.id}/verbs`, 'Save\thost:save\n');
  // a body in two writes, split inside a character
  const f = c.walk(root, ['wsys', `${w.id}`, 'body']);
  c.open(f, 1 | OTRUNC);
  const b = enc.encode('Saranos — a refuge\n');
  c.e.serve(new Writer().u32(f).u64(0).data(b.subarray(0, 9)).frame(T.write, 900));
  c.e.serve(new Writer().u32(f).u64(9).data(b.subarray(9)).frame(T.write, 901));
  c.clunk(f);
  assert.equal(w.body, 'Saranos — a refuge\n');
  assert.equal(w.dirty, false, 'what was read is clean');
  assert.equal(c.get(root, `wsys/${w.id}/body`), 'Saranos — a refuge\n');
  w.body += 'more';
  assert.equal(w.dirty, true);
  assert.equal(c.get(root, `wsys/${w.id}/dirty`), '1');
});

test('acme\'s new/ctl makes a window, and its ctl answers the id first', () => {
  const c = client();
  const root = c.attach('');
  const f = c.walk(root, ['new', 'ctl']);
  c.open(f, 0);
  const id = Number(c.read(f).trim().split(/\s+/)[0]);
  c.clunk(f);
  assert.ok(c.e.windows.has(id));
  assert.ok(c.e.windows.get(id).parent, 'placed as a tab');
});

test('a console read waits for a committed line, and a window that goes ends it', () => {
  const c = client();
  const w = c.e.make({ title: '/bin/rc', role: 'shell' });
  c.e.adopt(c.e.root, w);
  const view = c.attach(`${w.id}`);
  const cons = c.walk(view, ['cons']);
  c.open(cons, 2);
  // the program writes a prompt: the transcript grows
  c.write(cons, '% ');
  assert.equal(w.body, '% ');
  const t = c.rpc(T.read, (x) => x.u32(cons).u64(0).u32(8192));
  assert.equal(c.answer(t), null, 'nothing typed: the read waits');
  c.e.type(w, 'echo hi\n');
  const m = c.answer(t);
  assert.equal(dec.decode(m.bytes(m.u32())), 'echo hi\n');
  const t2 = c.rpc(T.read, (x) => x.u32(cons).u64(0).u32(8192));
  c.e.close(w);
  assert.throws(() => c.answer(t2), /window deleted/);
});

test('a flushed console read is answered no more', () => {
  const c = client();
  const w = c.e.make({ title: '/bin/rc', role: 'shell' });
  c.e.adopt(c.e.root, w);
  const cons = c.walk(c.attach(`${w.id}`), ['cons']);
  c.open(cons, 2);
  const t = c.rpc(T.read, (x) => x.u32(cons).u64(0).u32(8192));
  assert.ok(c.answer(c.rpc(T.flush, (x) => x.u16(t))) !== null, 'Rflush');
  c.e.type(w, 'late\n');
  assert.equal(c.answer(t), null);
  assert.deepEqual(w.input.map((b) => dec.decode(b)), ['late\n'], 'kept for the next read');
});

test('the interrupt posts a note to the window\'s group; the console read it ends is flushed', () => {
  const c = client();
  const root = c.attach('');
  const w = c.e.make({ title: '/bin/rc', role: 'shell' });
  c.e.adopt(c.e.root, w);
  c.put(root, `wsys/${w.id}/wctl`, 'set -pid 42\n');
  assert.equal(w.pid, 42);
  const cons = c.walk(c.attach(`${w.id}`), ['cons']);
  c.open(cons, 2);
  const t = c.rpc(T.read, (x) => x.u32(cons).u64(0).u32(8192));
  c.e.interrupt(w);
  assert.deepEqual(asked(c.e), ['echo interrupt >[2]/dev/null >/proc/42/notepg']);
  assert.equal(c.answer(t), null, 'the read waits on');
  // the note finds the reader waiting: the kernel flushes its read, and
  // what is typed next goes to the next read, not to the flushed one
  c.answer(c.rpc(T.flush, (x) => x.u16(t)));
  c.e.type(w, 'after\n');
  const t2 = c.rpc(T.read, (x) => x.u32(cons).u64(0).u32(8192));
  const r = c.answer(t2);
  assert.equal(dec.decode(r.bytes(r.u32())), 'after\n');
});

test('`events` gives IPNX what emca asks, as many bytes as it reads, and waits when there is none', () => {
  const c = client();
  const root = c.attach('');
  const ev = c.walk(root, ['wsys', '1', 'events']);
  c.open(ev, 0);
  const t = c.rpc(T.read, (x) => x.u32(ev).u64(0).u32(8192));
  assert.equal(c.answer(t), null);
  c.e.ask('emcaopen /etc/motd');
  const m = c.answer(t);
  assert.equal(dec.decode(m.bytes(m.u32())), 'emcaopen /etc/motd\n');
  // a byte at a time, as read(1) reads
  c.e.ask('ls');
  const one = c.answer(c.rpc(T.read, (x) => x.u32(ev).u64(0).u32(1)));
  assert.equal(dec.decode(one.bytes(one.u32())), 'l');
  const rest = c.answer(c.rpc(T.read, (x) => x.u32(ev).u64(0).u32(100)));
  assert.equal(dec.decode(rest.bytes(rest.u32())), 's\n');
});

test('Open makes a tab beside the window, and reuses a window already open on the name', () => {
  const c = client();
  const home = c.e.make({ title: '/usr/kitty', type: 'inode/directory', dir: '/usr/kitty' });
  c.e.adopt(c.e.root, home);
  const w = c.e.open(home, 'README');
  assert.equal(w.title, '/usr/kitty/README');
  assert.equal(home.parent.kind, 'tabs', 'the listing and the file share a rectangle');
  assert.equal(home.parent.current, w);
  assert.deepEqual(asked(c.e), [`cd /usr/kitty; emcaopen -w ${w.id} /usr/kitty/README >/dev/null`]);
  assert.equal(c.e.open(home, '/usr/kitty/README'), w, 'the same window');
  assert.deepEqual(asked(c.e), []);
});

test('Run sends the command to IPNX, its 1 and 2 to /output/<n>, and the log is both', () => {
  const c = client();
  const root = c.attach('');
  const home = c.e.make({ title: '/usr/kitty', type: 'inode/directory', dir: '/usr/kitty' });
  c.e.adopt(c.e.root, home);
  const out = c.e.run(home, 'ls -l');
  assert.equal(out.title, '/output/1/log');
  const [line] = asked(c.e);
  assert.equal(line, "@{cd /usr/kitty; rc -c 'ls -l' </dev/null >>/mnt/wsys/output/1/1 >>[2]/mnt/wsys/output/1/2; echo $status >/mnt/wsys/output/1/status} &");
  const say = (fd, text) => {
    const f = c.walk(root, ['output', '1', fd]);
    c.open(f, 1);
    c.write(f, text, 99);
    c.clunk(f);
  };
  say('1', 'README\n');
  say('2', "ls: x: 'x' file does not exist\n");
  c.put(root, 'output/1/status', 'ls 9: errors');
  assert.equal(out.body, "% cd /usr/kitty\n% ls -l\nREADME\nls: x: 'x' file does not exist\nexit ls 9: errors\n");
  assert.equal(out.status, 'exited ls 9: errors');
  assert.equal(c.get(root, 'output/1/1'), 'README\n');
  assert.equal(c.get(root, 'output/1/2'), "ls: x: 'x' file does not exist\n");
  assert.equal(c.get(root, 'output/1/cmd'), 'ls -l\n');
  assert.equal(c.get(root, 'output/1/dir'), '/usr/kitty\n');
});

test('Find selects every match and says how many; an address selects a line', () => {
  const c = client();
  const w = c.e.make({ title: '/x', body: 'cat\nconcat\ndog\n' });
  assert.deepEqual(c.e.find(w, 'cat'), [[0, 3], [7, 10]]);
  assert.equal(w.status, 'matches 2');
  assert.deepEqual(c.e.find(w, ':3'), [[11, 14]]);
});

test('a filter replaces the selection with what the command wrote', () => {
  const c = client();
  const root = c.attach('');
  const w = c.e.make({ title: '/x', role: 'edit', type: 'text/plain', body: 'b\na\n', dir: '/' });
  c.e.adopt(c.e.root, w);
  c.e.run(w, '|sort', '');
  assert.equal(c.get(root, `wsys/${w.id}/selection`), 'b\na\n');
  c.put(root, `wsys/${w.id}/replace`, 'a\nb\n');
  assert.equal(w.body, 'a\nb\n');
});

test('duplicate as column puts the copy beside it, as row below, as tab with it', () => {
  const c = client();
  const a = c.e.make({ title: '/a', dir: '/' });
  const col = c.e.make({ kind: 'column' });
  c.e.adopt(c.e.root, col);
  c.e.adopt(col, a);
  const b = c.e.duplicate(a, 'row');
  assert.equal(b.parent, col, 'below it, in its column');
  const d = c.e.duplicate(a, 'column');
  assert.equal(d.parent.kind, 'row', 'beside it: a row where it stood');
  assert.equal(a.parent, d.parent);
  const t = c.e.duplicate(a, 'tab');
  assert.equal(t.parent.kind, 'tabs');
  assert.equal(t.parent.current, t);
});

test('minimise and maximise move windows out of the allocation and back', () => {
  const c = client();
  const col = c.e.make({ kind: 'column' });
  c.e.adopt(c.e.root, col);
  const [a, b, d] = ['/a', '/b', '/d'].map((t) => {
    const w = c.e.make({ title: t });
    c.e.adopt(col, w);
    return w;
  });
  c.e.maximise(a);
  assert.deepEqual([a, b, d].map((w) => w.hidden), [false, true, true]);
  c.e.maximise(a);
  assert.deepEqual([a, b, d].map((w) => w.hidden), [false, false, false], 'and back');
  c.e.minimise(b);
  assert.equal(b.hidden, true);
});

test('closing a window closes what it holds, hangs up its program, and an emptied container goes', () => {
  const c = client();
  const col = c.e.make({ kind: 'column' });
  c.e.adopt(c.e.root, col);
  const sh = c.e.make({ title: '/bin/rc', role: 'shell', pid: 7 });
  c.e.adopt(col, sh);
  c.e.close(sh);
  assert.deepEqual(asked(c.e), ['echo hangup >[2]/dev/null >/proc/7/notepg']);
  assert.equal(c.e.windows.has(col.id), false);
});
