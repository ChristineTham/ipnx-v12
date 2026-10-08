// The browser host in a browser: Chromium, driven by Playwright, loads the
// page from serve.mjs — with the headers that give it shared memory — and is
// typed at through the page's own keyboard handling, as a person would.
//
//   sh hosts/web/build.sh && node hosts/web/test/browser.mjs
//
// It wants the playwright package and a Chromium it can launch; the tests
// under Node (web.test.mjs) want neither.

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

const { chromium } = await playwright();
const server = await serve(0);
const url = `http://localhost:${server.address().port}/`;
const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));

const screen = () => page.locator('#screen').innerText();
// The screen, once it shows `want` — or any of several, given as a list.
async function until(want, ms = 60000) {
  const t = Date.now();
  const wants = [want].flat();
  for (;;) {
    const s = await screen();
    if (wants.some((w) => s.includes(w))) return s;
    if (Date.now() - t > ms) throw new Error(`no ${JSON.stringify(want)} in:\n${s}\n${errors.join('\n')}`);
    await page.waitForTimeout(100);
  }
}

let failed = 0;
async function check(what, f) {
  const t = Date.now();
  try {
    await f();
    console.log(`ok - ${what} (${Date.now() - t} ms)`);
  } catch (e) {
    failed++;
    console.log(`not ok - ${what}\n${e.message}`);
  }
}

try {
  await page.goto(url);
  await check('the page is cross-origin isolated', async () => {
    if (!(await page.evaluate(() => crossOriginIsolated))) throw new Error('not isolated');
  });
  await check('it boots to a shell', () => until('% '));
  await check('a command typed at the page runs', async () => {
    await page.keyboard.type('echo hello from the page\n');
    await until('\nhello from the page\n');
  });
  await check('a pipeline', async () => {
    await page.keyboard.type('echo shouting | tr a-z A-Z\n');
    await until('SHOUTING');
  });
  await check('the page holds the line, and a backspace edits it', async () => {
    await page.keyboard.type('echo spelt');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('d\n');
    await until('% echo speld\nspeld\n');
  });
  await check('a fork, and a subshell with a namespace of its own', async () => {
    await page.keyboard.type('mkdir /tmp/alt; echo ALT >/tmp/alt/motd\n@{rfork n; bind /tmp/alt /etc; echo IN `{cat /etc/motd}}\necho OUT `{cat /etc/motd}\n');
    await until('OUT Saranos');
    if ((await screen()).includes('OUT ALT')) throw new Error('the bind escaped its process');
  });
  await check('threads sharing a memory serve 9P', async () => {
    await page.keyboard.type("plumber -p /dev/null; ls /mnt/plumb\n");
    await until('/mnt/plumb/send');
  });
  await check('^C interrupts a command, and the shell carries on', async () => {
    await page.keyboard.type('echo sleeping; sleep 30\n');
    await until('sleeping\n');
    await page.waitForTimeout(500);
    await page.keyboard.press('Control+c');
    await page.keyboard.type('echo after\n');
    // typed ahead of rc's new prompt, or after it
    await until(['% echo after\nafter\n', '\n% after\n'], 15000);
  });
  await check('a long list: the stack is deep enough', async () => {
    await page.keyboard.type("x=`{seq 3000}; echo $#x words\n");
    await until('3000 words');
  });
  await check('nothing went wrong in the page', async () => {
    if (errors.length) throw new Error(errors.join('\n'));
  });
} finally {
  await browser.close();
  server.close();
}
process.exit(failed ? 1 : 0);
