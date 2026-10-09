// emca in a browser: Chromium, driven by Playwright, loads the page — the
// system booted into its window manager — and reads what it draws, and acts
// on it as a person would: by clicking and typing.
//
//   sh hosts/web/build.sh && node hosts/web/test/emca-browser.mjs [check...]
//
// With names, it runs the page's own two checks and those whose names start
// with them, so the conformance suite can ask for one line at a time.

import { createRequire } from 'node:module';
import { execSync } from 'node:child_process';
import path from 'node:path';
import { serve } from '../serve.mjs';

async function playwright() {
  try {
    return await import('playwright');
  } catch {
    const global = execSync('npm root -g').toString().trim();
    return createRequire(path.join(global, 'x.js'))('playwright');
  }
}

const only = process.argv.slice(2);
const { chromium } = await playwright();
const server = await serve(0);
const url = `http://localhost:${server.address().port}/`;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));

// What the page draws: every window, with its title, its toolbar, what it
// shows and where; and every strip of tabs.
const seen = () =>
  page.evaluate(() => {
    const box = (e) => {
      const r = e.getBoundingClientRect();
      return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    };
    return {
      windows: [...document.querySelectorAll('article.window')].map((a) => ({
        id: a.dataset.id,
        kind: ['text', 'listing', 'shell'].find((k) => a.classList.contains(k)),
        title: a.querySelector('input.title').value,
        toolbar: [...a.querySelectorAll('.toolbar button')].map((b) => b.textContent),
        six: [...a.querySelectorAll('.six button')].map((b) => b.textContent),
        text: (a.querySelector('.cm-content') ?? a.querySelector('.body')).innerText,
        box: box(a),
      })),
      tabs: [...document.querySelectorAll('section.container[data-kind="tabs"]')].map((s) => ({
        names: [...s.querySelectorAll(':scope > nav.strip .tab .name')].map((b) => b.textContent),
        box: box(s),
      })),
      global: [...document.querySelectorAll('header.global .toolbar button')].map((b) => b.textContent),
    };
  });
const titled = (s, t) => s.windows.find((w) => w.title === t);
const win = (id) => page.locator(`article.window[data-id="${id}"]`);

// Wait until `f(seen)` is truthy, or fail with what the page showed.
async function until(what, f, ms = 60000) {
  const t = Date.now();
  for (;;) {
    const s = await seen();
    const v = f(s);
    if (v) return v;
    if (Date.now() - t > ms) throw new Error(`waited ${ms} ms for ${what}:\n${JSON.stringify(s, null, 1)}\n${errors.join('\n')}`);
    await page.waitForTimeout(100);
  }
}

let failed = 0;
async function check(what, f) {
  if (only.length && !only.some((o) => what.startsWith(o)) && !f.always) return;
  const t = Date.now();
  try {
    await f();
    console.log(`ok - ${what} (${Date.now() - t} ms)`);
  } catch (e) {
    failed++;
    console.log(`not ok - ${what}\n${e.message}`);
  }
}
const always = (f) => Object.assign(f, { always: true });

const assert = (ok, msg) => {
  if (!ok) throw new Error(msg);
};

try {
  await page.goto(url);
  await check('the page is cross-origin isolated', always(async () => {
    assert(await page.evaluate(() => crossOriginIsolated), 'not isolated');
  }));
  await check('it boots into emca, and IPNX fills every window', always(async () => {
    await until('every window filled', (s) => {
      const rc = titled(s, '/bin/rc');
      return titled(s, '/home')?.text.includes('README') && rc?.text.includes('% ') && s.tabs[0]?.names.length === 3;
    }, 120000);
    await until('/etc/motd', (s) => titled(s, '/etc/motd')?.text.includes('Saranos.'));
  }));
  // The layout as /type/inode/system/layout gives it, drawn as the
  // compositor allocates it at this width (compositor.md): presentation,
  // which the conformance suite does not ask about.
  await check('the layout: the listing, the three tabs, and rc below them', async () => {
    const s = await seen();
    const home = titled(s, '/home');
    const rc = titled(s, '/bin/rc');
    const [tabs] = s.tabs;
    assert(home.kind === 'listing', `/home is a ${home.kind}`);
    assert(rc.kind === 'shell', `/bin/rc is a ${rc.kind}`);
    assert(tabs.names.join(' ') === 'motd tour README', `the tabs are ${tabs.names}`);
    assert(home.box.right <= tabs.box.left, 'the listing is not on the left');
    assert(rc.box.top >= tabs.box.bottom - 1, 'rc is not below the tabs');
    assert(rc.box.left >= home.box.right, 'rc is not beside the listing');
    // the tab showing is the first, /etc/motd, and its text is the file's
    const motd = titled(s, '/etc/motd');
    assert(motd?.kind === 'text', `/etc/motd is a ${motd?.kind}`);
    assert(s.global.includes('Save All') && s.global.includes('New Shell'), `the global toolbar is ${s.global}`);
  });

  await check('several things at once: acting on one leaves the others alone', async () => {
    const before = await seen();
    const home = titled(before, '/home');
    const motd = titled(before, '/etc/motd');
    const rc = titled(before, '/bin/rc');
    // a command, typed into rc
    await win(rc.id).locator('.body').click();
    await page.keyboard.type('echo typed into rc\n');
    await until('rc answers', (s) => titled(s, '/bin/rc').text.includes('\ntyped into rc\n'));
    // an edit to the text in the tab showing
    await win(motd.id).locator('.cm-content').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('an added line');
    // the tour's tab, chosen, and then closed
    await page.locator('section.container[data-kind="tabs"] .tab', { hasText: 'tour' }).locator('button.name').click();
    const tour = await until('the tour, shown', (s) => titled(s, '/bin/tour'));
    assert(tour.text.includes('the tour'), 'the tour shows its transcript');
    await page.locator('section.container[data-kind="tabs"] .tab', { hasText: 'tour' }).locator('button.x').click();
    const after = await until('the tour, closed', (s) => !titled(s, '/bin/tour') && s.tabs[0]?.names.length === 2 && s);
    // and what each shows now: only what was done to it changed
    assert(after.tabs[0].names.join(' ') === 'motd README', `the tabs are ${after.tabs[0].names}`);
    assert(titled(after, '/home').text === home.text, 'the listing changed');
    const rc2 = titled(after, '/bin/rc');
    assert(rc2.text.startsWith(rc.text.replace(/\n?$/, '')), 'rc lost its transcript');
    assert(!rc2.text.includes('an added line'), 'the edit went to rc');
    await page.locator('section.container[data-kind="tabs"] .tab', { hasText: 'motd' }).locator('button.name').click();
    const motd2 = await until('/etc/motd again', (s) => titled(s, '/etc/motd'));
    assert(motd2.text.includes('an added line'), 'the edit is not in /etc/motd');
    assert(!motd2.text.includes('typed into rc'), 'what rc was given went to /etc/motd');
    assert(motd2.toolbar.includes('Save'), 'the edited text offers no Save');
    await page.locator('section.container[data-kind="tabs"] .tab', { hasText: 'README' }).locator('button.name').click();
    const readme = await until('/home/README', (s) => titled(s, '/home/README'));
    assert(readme.text.includes('Welcome home') && !readme.text.includes('an added line'), 'README changed');
    assert(!readme.toolbar.includes('Save'), 'README, untouched, offers Save');
  });

  await check('what you can do depends on what it is: text, a listing and a shell offer different verbs', async () => {
    await page.locator('section.container[data-kind="tabs"] .tab', { hasText: 'README' }).locator('button.name').click();
    const s = await until('/home/README, shown and editable', (s) => titled(s, '/home/README')?.toolbar.includes('Undo') && s);
    const text = titled(s, '/home/README');
    const listing = titled(s, '/home');
    const shell = titled(s, '/bin/rc');
    assert(text.toolbar.join(' ') === 'Revert Undo Redo', `text offers ${text.toolbar}`);
    assert(listing.toolbar.join(' ') === 'Revert', `a listing offers ${listing.toolbar}`);
    assert(shell.toolbar.join(' ') === 'Interrupt', `a shell offers ${shell.toolbar}`);
    // and each does what it says: Interrupt ends what the shell runs...
    await win(shell.id).locator('.body').click();
    await page.keyboard.type('echo sleeping; sleep 30; echo slept\n');
    await until('sleeping', (s) => titled(s, '/bin/rc').text.includes('\nsleeping\n'));
    await page.waitForTimeout(300);
    const t = Date.now();
    await win(shell.id).locator('.toolbar button', { hasText: 'Interrupt' }).click();
    await win(shell.id).locator('.body').click();
    await page.keyboard.type('echo after\n');
    // typed ahead of rc's new prompt, or after it
    await until('after', (s) => /(\n|% )after\n/.test(titled(s, '/bin/rc').text), 20000);
    // what was typed is in the transcript; what it would have printed is not
    assert(Date.now() - t < 15000 && !(await seen()).windows.some((w) => /(^|\n)slept(\n|$)/.test(w.text)), 'the sleep ran its course');
    // ...a name in the listing opens it...
    await win(listing.id).locator('.name', { hasText: 'profile/' }).click();
    const profile = await until('profile/, opened', (s) => titled(s, '/home/profile')?.text.includes('start.rc') && titled(s, '/home/profile'));
    assert(profile.kind === 'listing', `profile/ is a ${profile.kind}`);
    // ...and Undo takes back an edit in text
    await win(text.id).locator('.cm-content').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('undone');
    await until('the edit', (s) => titled(s, '/home/README').text.includes('undone'));
    await win(text.id).locator('.toolbar button', { hasText: 'Undo' }).click();
    await until('undone', (s) => !titled(s, '/home/README').text.includes('undone'));
  });

  await check('nothing went wrong in the page', always(async () => {
    assert(!errors.length, errors.join('\n'));
  }));
} finally {
  await browser.close();
  server.close();
}
process.exit(failed ? 1 : 0);
