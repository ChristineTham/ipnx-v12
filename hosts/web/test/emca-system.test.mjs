// emca with the system: the kernel booted under Node, its window manager's
// model attached as the page attaches it — and no page. What a person would
// see is read from the model, and what they would do is done to it.
//
//   bash userspace/mk.sh && sh hosts/web/build.sh && node --test hosts/web/test/emca-system.test.mjs

import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';

const dist = new URL('../dist/', import.meta.url);
if (!fs.existsSync(new URL('kernel.wasm', dist))) throw new Error('no hosts/web/dist — run hosts/web/build.sh');
const { boot } = await import(new URL('saranos.mjs', dist));
const { Emca } = await import(new URL('emca.mjs', dist));

// Boot with emca; answer the model and the system.
function session() {
  const emca = new Emca();
  let console_ = '';
  const dec = new TextDecoder();
  const sys = boot({ site: dist, out: (b) => (console_ += dec.decode(b, { stream: true })), emca });
  return { emca, sys, console: () => console_ };
}

// Wait until `f()` is truthy, or fail saying what was being waited for.
async function until(what, f, ms = 60000) {
  const t = Date.now();
  for (;;) {
    const v = f();
    if (v) return v;
    if (Date.now() - t > ms) throw new Error(`waited ${ms} ms for ${what}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

const leaves = (e) => [...e.windows.values()].filter((w) => w.leaf && w !== e.root);
const byTitle = (e, t) => leaves(e).find((w) => w.title === t);

test('the system boots into emca: the layout, every window filled by IPNX', async () => {
  const { emca: e, sys, console: con } = session();
  try {
    await until('the layout', () => e.root.children.length === 2, 90000);
    const home = await until('/home listed', () => byTitle(e, '/home')?.type === 'inode/directory' && byTitle(e, '/home'), 90000);
    await until('/home has its README', () => home.body.split('\n').includes('README'));
    // a window is filled when its role is written, which emcaopen does last
    const filled = (t, text) => byTitle(e, t)?.role && byTitle(e, t).body.includes(text) && byTitle(e, t);
    const motd = await until('/etc/motd read', () => filled('/etc/motd', 'Saranos.'));
    assert.equal(motd.type, 'text/plain');
    // the root's files are the attaching user's, as u9fs serves them
    assert.equal(motd.role, 'edit');
    assert.match(motd.verbs, /Revert/);
    const readme = await until('/home/README read', () => filled('/home/README', 'Welcome home'));
    assert.equal(readme.role, 'edit', 'kitty\'s own file');
    const tour = await until('the tour, run', () => byTitle(e, '/bin/tour')?.body.includes('the tour') && byTitle(e, '/bin/tour'));
    assert.equal(tour.role, 'shell');
    const rc = await until('rc, prompting', () => leaves(e).find((w) => w.title === '/bin/rc' && w.body.includes('% ')));
    assert.equal(rc.role, 'shell');
    assert.ok(rc.pid > 0, 'its note group is known');
    // the structure: the listing, and beside it the tabs above rc
    const [left, col] = e.root.children;
    assert.equal(left, home);
    assert.equal(col.kind, 'column');
    assert.equal(col.children[0].kind, 'tabs');
    assert.deepEqual(col.children[0].children.map((w) => w.title), ['/etc/motd', '/bin/tour', '/home/README']);
    assert.equal(col.children[1], rc);
    assert.match(con(), /service: emca started/);
  } finally {
    sys.stop();
  }
});

test('what is typed into a shell window runs, and its output is the transcript', async () => {
  const { emca: e, sys } = session();
  try {
    const rc = await until('rc, prompting', () => leaves(e).find((w) => w.title === '/bin/rc' && w.body.includes('% ')), 90000);
    e.type(rc, 'echo hello from a window; cat /dev/window/title\n');
    await until('the answer', () => rc.body.includes('hello from a window\n/bin/rc'));
    // its /dev is its window's, and /dev/wsys holds every window's
    e.type(rc, 'ls /dev/wsys | wc -l\n');
    await until('the windows, counted', () => /\n\s*\d+\n% $/.test(rc.body));
  } finally {
    sys.stop();
  }
});

test('the interrupt ends what the shell is running, and the shell carries on', async () => {
  const { emca: e, sys } = session();
  try {
    const rc = await until('rc, prompting', () => leaves(e).find((w) => w.title === '/bin/rc' && w.body.includes('% ')), 90000);
    e.type(rc, 'echo sleeping; sleep 30; echo slept\n');
    await until('sleeping', () => rc.body.includes('sleeping\n'));
    await new Promise((r) => setTimeout(r, 300));
    const t = Date.now();
    e.interrupt(rc);
    e.type(rc, 'echo after\n');
    await until('after', () => rc.body.includes('after\n'), 20000);
    assert.ok(Date.now() - t < 15000, 'the sleep did not run its course');
    // what was typed is in the transcript; what it would have printed is not
    assert.ok(!/(^|\n)slept\n/.test(rc.body));
    // and an interrupt while rc waits for a line: a new prompt
    const n = rc.body.length;
    e.interrupt(rc);
    e.type(rc, 'echo still here\n');
    await until('still here', () => rc.body.slice(n).includes('still here\n'), 20000);
  } finally {
    sys.stop();
  }
});

test('Open, Run, Save, Revert and Edit do what they mean, through IPNX', async () => {
  const { emca: e, sys } = session();
  try {
    const home = await until('/home', () => byTitle(e, '/home')?.body.includes('README') && byTitle(e, '/home'), 90000);
    // Run: output in /output/1/log, and its status
    const out = e.run(home, 'echo ran in `{pwd}');
    await until('the output', () => out.body.includes('ran in /home\n') && out.status === 'exited');
    // Open a name that does not exist: a window on it, which Save makes
    const note = e.open(home, 'note.txt');
    assert.equal(note.title, '/home/note.txt');
    await until('its type', () => note.type === 'text/plain' && note.role === 'edit');
    note.body = 'first line\nsecond line\n';
    assert.ok(note.dirty);
    e.save(note);
    await until('saved', () => !note.dirty);
    e.revert(home);
    await until('the listing again, with the file', () => home.body.split('\n').includes('note.txt'));
    // Edit: a sam command over the text
    e.edit(note, ',x/line/c/LINE/');
    await until('edited', () => note.body === 'first LINE\nsecond LINE\n');
    assert.ok(note.dirty, 'an edit is unsaved until Save');
    // and a filter: the selection through a command
    e.run(note, '|tr a-z A-Z', 'first LINE');
    await until('filtered', () => note.body.startsWith('FIRST LINE\n'));
  } finally {
    sys.stop();
  }
});
