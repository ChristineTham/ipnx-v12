// WASI programs in a browser (P10): Chromium, driven by Playwright, loads
// the console page from serve.mjs and is typed at, as browser.mjs types at
// the shell — gohello, the previous demo's Go program built for wasip1, and
// CPython's own WASI build, each run by www/wasi.mjs over the process's
// namespace. Name checks to run only those; the page's isolation, its boot
// and its errors are always checked.
//
//   sh hosts/web/build.sh && node hosts/web/test/wasi-browser.mjs [check...]
//
// It wants the playwright package and a Chromium it can launch, and the
// programs in the root: gohello is built by userspace/mk.sh with go, and
// Python by userspace/pkg/python/mk.sh.

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
const url = `http://localhost:${server.address().port}/console.html`;
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
async function check(what, f, always = false) {
  if (only.length && !only.some((o) => what.startsWith(o)) && !always) return;
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
  }, true);
  await check('it boots to a shell', () => until('% '), true);
  await check('a Go program built for wasip1 runs, unmodified', async () => {
    await page.keyboard.type('gohello\n');
    await until('Hello Kitty — from Go (GOOS=wasip1, unmodified)');
  });
  await check('Python imports from its library and computes', async () => {
    await page.keyboard.type(`python3 -c 'import json; print(json.dumps({"sum": sum(range(10))}))'\n`);
    await until('{"sum": 45}', 120000);
  });
  await check('Python reads its prompt from the console', async () => {
    await page.keyboard.type('python3 -q\n');
    await until('>>> ', 120000);
    await page.keyboard.type('print(6*7)\n');
    await until('\n42\n');
    await page.keyboard.type('exit()\necho back in rc\n');
    // typed ahead of rc's new prompt, or after it
    await until(['% echo back in rc\nback in rc\n', '\n% back in rc\n']);
  });
  await check('^C ends a WASI program in a loop, and the shell carries on', async () => {
    await page.keyboard.type(`python3 -c 'print("looping", flush=True); exec("while True: pass")'\n`);
    await until('looping\n', 120000);
    await page.waitForTimeout(500);
    await page.keyboard.press('Control+c');
    await page.keyboard.type('echo after the loop\n');
    // typed ahead of rc's new prompt, or after it
    await until(['% echo after the loop\nafter the loop\n', '\n% after the loop\n', '\nafter the loop\n'], 15000);
  });
  await check('nothing went wrong in the page', async () => {
    if (errors.length) throw new Error(errors.join('\n'));
  }, true);
} finally {
  await browser.close();
  server.close();
}
process.exit(failed ? 1 : 0);
