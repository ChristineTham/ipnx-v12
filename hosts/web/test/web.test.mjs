// The browser host's tests: the whole system — the kernel compiled to
// wasm32 in a worker, each process a worker — booted under Node, whose
// worker_threads are Web Workers here, and typed at the way a person would.
// They are hosts/ipnx's typed tests, run on this machine: the same keys,
// the same answers.
//
//   bash userspace/mk.sh && sh hosts/web/build.sh && node --test hosts/web/test/

import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';

const dist = new URL('../dist/', import.meta.url);
const repo = new URL('../../../', import.meta.url);
if (!fs.existsSync(new URL('kernel.wasm', dist))) throw new Error('no hosts/web/dist — run hosts/web/build.sh');
const { boot } = await import(new URL('saranos.mjs', dist));

const enc = new TextEncoder();

// Boot, type `keys`, end the input, and answer what the screen showed when
// the system was over. A `\x03` in the keys is the interrupt key, pressed
// once the screen shows `mark` and has been still for a moment.
async function typing(keys, { cmd = [], mark = '', limit = 120000 } = {}) {
  let screen = '';
  const dec = new TextDecoder();
  const sys = boot({ site: dist, cmd, out: (b) => (screen += dec.decode(b, { stream: true })) });
  const timer = setTimeout(() => sys.stop(), limit);
  const parts = keys.split('\x03');
  for (let i = 0; i < parts.length; i++) {
    if (i > 0) {
      // the screen shows the mark, and is still
      for (let last = null; !(screen.includes(mark) && screen === last); ) {
        last = screen;
        await new Promise((r) => setTimeout(r, 500));
      }
      sys.interrupt();
    }
    sys.type(enc.encode(parts[i]));
  }
  sys.end();
  const r = await sys.done;
  clearTimeout(timer);
  assert.ok(r.ok, `the system failed: ${r.status}\n${screen}`);
  return screen;
}

// The seconds between the first two `date -n` lines a script printed.
function seconds(out) {
  const t = out
    .split('\n')
    .map((l) => Number(l.trim().split(/\s+/).pop()))
    .filter((n) => n > 1e9);
  assert.ok(t.length >= 2, `two clock readings: ${out}`);
  return t[1] - t[0];
}

// ---- what the two halves agree on ----

// Every stub in sys.c is a call a process makes, by the number sys.h gives
// it — the kernel's `sysno` — or one proc.mjs answers itself (asyncify's).
test('every call a program can make has its number', async () => {
  const sysc = fs.readFileSync(new URL('userspace/sys/src/libc/wasm/sys.c', repo), 'utf8');
  const lib = fs.readFileSync(new URL('kernel/src/lib.rs', repo), 'utf8');
  const { CALLS } = await import(new URL('mailbox.mjs', dist));
  const sysno = Object.fromEntries([...lib.matchAll(/pub const ([A-Z0-9]+): u32 = (\d+);/g)].map((m) => [m[1], Number(m[2])]));
  const names = [...sysc.matchAll(/^SYS\((\w+)\)/gm)].map((m) => m[1]);
  assert.ok(names.length >= 30, `read ${names.length} stubs`);
  const kernelname = { _stat: 'OLDSTAT', _fstat: 'OLDFSTAT' };
  for (const n of names) {
    if (n === 'setjmp' || n === 'longjmp') continue;
    const k = kernelname[n] ?? n.toUpperCase();
    assert.equal(CALLS[n], sysno[k], `${n}: ${CALLS[n]} here, ${sysno[k]} in the kernel`);
  }
});

// The mailbox's numbers are js.rs's.
test('the mailbox says what the kernel reads', async () => {
  const js = fs.readFileSync(new URL('hosts/web/src/js.rs', repo), 'utf8');
  const { ev, rep } = await import(new URL('mailbox.mjs', dist));
  const mod = (name) => {
    const body = js.slice(js.indexOf(`pub mod ${name} {`));
    return Object.fromEntries([...body.slice(0, body.indexOf('\n}')).matchAll(/pub const (\w+): i32 = (\d+);/g)].map((m) => [m[1], Number(m[2])]));
  };
  assert.deepEqual(mod('ev'), ev);
  assert.deepEqual(mod('rep'), rep);
});

// ---- the system ----

test('it boots to a shell', async () => {
  const out = await typing('echo it booted\n');
  assert.match(out, /% /);
  assert.match(out, /it booted\n/);
});

test('a command on the host command line runs through init', async () => {
  let out = await typing('', { cmd: ['echo', 'hello', "it's here"] });
  assert.match(out, /hello it's here\n/);
  out = await typing('', { cmd: ['cat', '/env/objtype'] });
  assert.match(out, /init: starting \/bin\/rc\nwasm/);
});

test('the machine names itself', async () => {
  const out = await typing('echo $terminal\ncat /dev/config\n');
  assert.match(out, /wasm hosts\/web\/src\/lib\.rs\n/);
  assert.match(out, /# hosts\/web\/src\/lib\.rs - Saranos in a browser\ndev\n/);
});

test('ls shows both halves of the root', async () => {
  const out = await typing('ls /\n');
  for (const name of ['dev', 'proc', 'srv', 'etc', 'home', 'lib', 'profile', 'usr', 'wasm']) {
    assert.match(out, new RegExp(`/${name}\n`), name);
  }
});

test('cat /etc/motd', async () => {
  assert.match(await typing('cat /etc/motd\n'), /Saranos\./);
});

test('a pipeline is two processes and the shell waits for them', async () => {
  const out = await typing('echo shouting | tr a-z A-Z\ncat /proc/1/status\n');
  assert.match(out, /SHOUTING/);
  assert.match(out, /Await/);
});

test('a sleeping process comes back', async () => {
  assert.match(await typing('sleep 1\necho awake\n'), /awake/);
});

test('a forked child may sleep before it execs', async () => {
  const out = await typing("mkdir -p /tmp/p; bind '#|' /tmp/p\ncat /bin/echo >/tmp/p/data &\n/tmp/p/data1 it ran\necho after\n");
  assert.match(out, /it ran\n/);
  assert.match(out, /after\n/);
});

test('sed edits, and a bad expression unwinds by longjmp', async () => {
  const out = await typing("echo hello world | sed s/world/kitty/\necho x | sed 's/[/y/'\nseq 5 | sed -n '2,3p'\necho still here\n");
  assert.match(out, /hello kitty\n/);
  assert.match(out, /sed: r\.e\.-using command garbled/);
  assert.match(out, /2\n3\n/);
  assert.match(out, /still here/);
});

test('ed recovers from errors by longjmp, more than once', async () => {
  const out = await typing('echo a >/tmp/ed1; {echo zz; echo zz; echo 1p; echo q} | ed /tmp/ed1; rm /tmp/ed1\n');
  assert.match(out, /2\n\?\n\?\na\n/);
});

test('a subshell is a copy', async () => {
  assert.match(await typing('x=parent; @{x=child; echo in $x}; echo out $x\n'), /in child\nout parent\n/);
});

test('fork returns twice', async () => {
  const out = await typing('time echo forked\n');
  assert.match(out, /forked\n/);
  assert.match(out, /r \t echo forked/);
});

// libthread's procs share one memory (RFMEM), and serve 9P to the others.
test('plumber serves and plumb delivers', async () => {
  const out = await typing(
    "plumber -p /dev/null\n{echo 'type is text'; echo 'data matches hello'; echo 'plumb to edit'} >/mnt/plumb/rules\nls /mnt/plumb\ncat /mnt/plumb/edit >/tmp/plumbed &\nfor(i in 1 2 3 4 5 6 7 8 9 10) if(! test -s /tmp/plumbed){ plumb -d edit -s me hello; sleep 1 }\ncat /tmp/plumbed\n",
  );
  assert.match(out, /\/mnt\/plumb\/edit\n\/mnt\/plumb\/rules\n\/mnt\/plumb\/send\n/);
  assert.match(out, /me\nedit\n\/\ntext\n\n5\nhello/);
});

test('an rc script runs by name', async () => {
  const out = await typing("echo '#!/bin/rc' >/tmp/s.rc; echo 'echo script $0 $*' >>/tmp/s.rc; chmod +x /tmp/s.rc\n/tmp/s.rc a 'b c'\n");
  assert.match(out, /script \/tmp\/s\.rc a b c\n/);
});

test('exec of something that is not a module fails, and the system goes on', async () => {
  const out = await typing('echo junk >/tmp/j\nchmod +x /tmp/j\n/tmp/j || echo refused\necho still here\n');
  assert.match(out, /refused/);
  assert.match(out, /still here/);
});

// The page runs no host commands (docs/saranos.md, The host's resources):
// `os` loads, and the page answers its call to the host as Plan 9 answers
// a number with no `systab` entry (`pc/trap.c:716`); the shell goes on.
test('os is a call the page does not have, and the system goes on', async () => {
  const out = await typing('os echo hello </dev/null\necho still here\n');
  assert.match(out, /bad sys call number 54/);
  assert.doesNotMatch(out, /^hello$/m);
  assert.match(out, /still here/);
});

test('a process in a tight loop does not stop the system', async () => {
  assert.match(await typing('{while(~ 1 1) x=1} &\necho kill >/proc/$apid/ctl\necho still here\n'), /still here/);
});

test('stop stops a process in a tight loop', async () => {
  const out = await typing("awk 'BEGIN{for(;;)x++}' &\nsleep 1\necho stop >/proc/$apid/ctl\nsed 's/  */ /g' /proc/$apid/status\necho kill >/proc/$apid/ctl\necho still here\n");
  assert.match(out, /awk kitty Stopped/);
  assert.match(out, /still here/);
});

test('if not in a script', async () => {
  const out = await typing("echo 'if(~ x y) s=1' >/tmp/ifnot.rc\necho 'if not s=2' >>/tmp/ifnot.rc\necho 'echo s is $s' >>/tmp/ifnot.rc\nrc /tmp/ifnot.rc\n");
  assert.match(out, /s is 2/);
});

// A process's stack is Plan 9's size: rc recurses once a word.
test('rc parses a long list', async () => {
  const out = await typing("{echo -n 'x=('; seq 5000 | tr '\\012' ' '; echo ')'; echo 'echo $#x words'} >/tmp/long.rc\n. /tmp/long.rc\n");
  assert.match(out, /5000 words/);
});

test('rc catches a note with its own handler', async () => {
  const out = await typing('fn sigint { echo caught }\necho interrupt >/proc/$pid/note\necho after\n');
  assert.match(out, /caught/);
  assert.match(out, /after/);
});

test('kill ends a sleeping process at once', async () => {
  const out = await typing('{exec sleep 30} &\ndate -n\necho kill >/proc/$apid/ctl\nwait\ndate -n\necho done\n');
  assert.match(out, /done/);
  assert.ok(seconds(out) < 20, out);
});

// The pc is asked of the process, which answers from its own stack.
test('startsyscall shows the next call, with the pc it was made from', async () => {
  const out = await typing('{exec sleep 2} &\necho stop >/proc/$apid/ctl\necho startsyscall >/proc/$apid/ctl\ncat /proc/$apid/syscall; echo\necho start >/proc/$apid/ctl\nwait\necho done\n');
  const line = out.split('\n').find((l) => l.includes(' sleep Sleep '));
  assert.ok(line, out);
  const w = line.trim().split(/\s+/);
  const at = w.indexOf('Sleep');
  assert.ok(parseInt(w[at + 1], 16) > 0, `a pc: ${line}`);
  assert.equal(w[at + 2], '1000', line);
  assert.match(out, /done/);
});

test('the interrupt key interrupts a command, and the shell carries on', async () => {
  const out = await typing('sleep 0; echo sleeping; date -n; sleep 30\n\x03date -n; echo after\n', { mark: 'sleeping\n' });
  assert.match(out, /after/);
  assert.ok(seconds(out) < 20, out);
});

test('the interrupt key interrupts a command reading the console', async () => {
  assert.match(await typing('cat </dev/null; echo reading; cat\n\x03echo after\n', { mark: 'reading\n' }), /after/);
});

test('sysstat counts interrupts and calls', async () => {
  const out = await typing('sleep 1\ncat /dev/sysstat\n');
  const line = out.split('\n').find((l) => l.trim().split(/\s+/).length >= 10);
  const v = line.trim().split(/\s+/).slice(-10).map(Number);
  assert.ok(v[2] > 0, `interrupts: ${line}`);
  assert.ok(v[3] > 0, `syscalls: ${line}`);
});

test('date reads the clock every time', async () => {
  const out = await typing('date -n; sleep 1; date -n; sleep 1; date -n\n');
  const now = Date.now() / 1000;
  const v = out.split(/[^0-9-]+/).map(Number).filter((n) => n > 1e9);
  assert.equal(v.length, 3, out);
  for (const t of v) assert.ok(Math.abs(now - t) < 120, `${t} against ${now}`);
});

test("rc's own control flow, substitution and status", async () => {
  const out = await typing('x=world\nfor(i in a b c) echo $i $x\necho `{echo through}\ncat /nothing\necho after [$status]\necho ok\necho then [$status]\ncat /dev/pid\necho done\n');
  assert.match(out, /a world\nb world\nc world\n/);
  assert.match(out, /through\n/);
  assert.match(out, /: can't open \/nothing/);
  assert.match(out, /after \[cat /);
  assert.match(out, /then \[\]/);
  assert.match(out, /done\n/);
});

test('processes have their own namespaces', async () => {
  const out = await typing('mkdir /tmp/alt\necho ALT >/tmp/alt/motd\n@{rfork n; bind /tmp/alt /etc; echo IN `{cat /etc/motd}}\necho OUT `{cat /etc/motd}\n');
  assert.match(out, /IN ALT/);
  assert.doesNotMatch(out, /OUT ALT/);
  assert.match(out, /OUT Saranos/);
});

// What is written stays in the page's tree for the session.
test('a file written is read back', async () => {
  const out = await typing('echo kept >/tmp/note\ncat /tmp/note\nls -l /tmp/note\nrm /tmp/note\ncat /tmp/note\n');
  assert.match(out, /kept\n/);
  assert.match(out, / 5 .* \/tmp\/note\n/);
  assert.match(out, /can't open \/tmp\/note: '\/tmp\/note' does not exist/);
});

test('a package installs as a bind, and a subshell can have its own', async () => {
  const out = await typing(
    "mkdir -p /usr/kitty/repo/src/hello/wasm/bin\ncd /usr/kitty/repo\necho '#!/bin/rc' >src/hello/wasm/bin/hello\necho 'echo hello from a package' >>src/hello/wasm/bin/hello\nchmod +x src/hello/wasm/bin/hello\necho 'pkg=hello version=1.0' >src/hello/pkg.cfg\n{echo +; echo '\tpkg.cfg'; echo '\twasm'; echo '\t\tbin'; echo '\t\t\thello'} >proto\ndisk/mkfs -a -s src/hello proto >hello-1.0.mkfs >[2]/dev/null\nsum=`{sha1sum -2 256 hello-1.0.mkfs}\necho 'pkg=hello version=1.0 sha256='$sum(1)' file=hello-1.0.mkfs' >index\necho 'bind /usr/kitty/repo /n/pkg' >/profile/repository\ncd /\n@{rfork n; pkg install -n hello >/dev/null; echo IN `{hello}}\necho OUT; hello\n",
  );
  assert.match(out, /IN hello from a package/);
  const outside = out.split('OUT').pop();
  assert.doesNotMatch(outside, /hello from a package/);
  assert.match(outside, /does not exist/);
});

test('ghostscript runs, with its fonts', async () => {
  const out = await typing("gs -q -dNODISPLAY -c '1 2 add ==' -c '/Times-Roman findfont 10 scalefont setfont (Hi) stringwidth pop ==' -c quit\n");
  const last = (l) => l.trim().split(/\s+/).pop();
  assert.ok(out.split('\n').some((l) => last(l) === '3'), out);
  assert.ok(out.split('\n').some((l) => { const w = Number(last(l)); return w >= 9.5 && w < 10.5; }), out);
});

// ---- WASI programs (P10; docs/architecture.md, A WASI binary runs natively) ----
// wasi.mjs: WASI preview 1 over the process's own calls, with its namespace
// as the program's files — what hosts/ipnx's `wasi` and `python` tests ask of
// the terminal. Each program is the build's, and a test without it says so.

const inroot = (p) => fs.existsSync(new URL(`root/${p}`, dist));
const gohello = fs.readdirSync(new URL('root/pkg/system/', dist)).some((v) => inroot(`pkg/system/${v}/wasm/bin/gohello`));
const nogo = !gohello && 'no gohello in the root: userspace/mk.sh builds it with go';
const nopython = !inroot('pkg/python') && 'no /pkg/python in the root: userspace/pkg/python/mk.sh';
// typing is not echoed: the output follows the prompt
const line = (out, want) => out.split('\n').some((l) => l.replace(/^(% )+/, '') === want);

test('a Go program built for wasip1 runs, unmodified', { skip: nogo }, async () => {
  const out = await typing('gohello\n');
  assert.ok(line(out, 'Hello Kitty — from Go (GOOS=wasip1, unmodified)'), out);
});

test('Python imports from its library and computes', { skip: nopython }, async () => {
  const out = await typing(
    'python3 -c \'import json, sys; print(json.dumps({"answer": 6*7}), sys.prefix)\'\n' +
      'python3 -c \'open("/tmp/from-python", "w").write("written by python\\n")\'\n' +
      'cat /tmp/from-python\n',
  );
  assert.ok(line(out, '{"answer": 42} /sys'), out);
  assert.ok(line(out, 'written by python'), out);
});

test('Python reads its prompt from the console', { skip: nopython }, async () => {
  assert.match(await typing('python3 -q\nprint(sum(range(10)))\n'), /45/);
});

// "B: the namespace" (Christine, 2026-10-09): a bind in a subshell is what
// the program there opens, and not what one outside it does.
test("a WASI program's files are its process's namespace", { skip: nopython }, async () => {
  const out = await typing(
    'mkdir /tmp/alt; echo ALT >/tmp/alt/motd\n' +
      "@{rfork n; bind /tmp/alt /etc; python3 -c 'print(\"IN\", open(\"/etc/motd\").read().strip())'}\n" +
      "python3 -c 'print(\"OUT\", open(\"/etc/motd\").read().split()[0])'\n",
  );
  assert.ok(line(out, 'IN ALT'), out);
  assert.ok(line(out, 'OUT Saranos.'), out);
});

// _exit.c's status: the number, and none for 0
test("a WASI program's exit status is APE's", { skip: nopython }, async () => {
  const out = await typing("python3 -c 'import sys; sys.exit(5)'; echo status $status\npython3 -c 'pass' && echo succeeded\n");
  assert.match(out, /status python3 \d+: 5\n/);
  assert.ok(line(out, 'succeeded'), out);
});

// A WASI program has no note handler: the interrupt ends it, at the clock.
test('the interrupt key ends a WASI program in a loop, and the shell carries on', { skip: nopython }, async () => {
  const out = await typing("python3 -c 'print(\"looping\", flush=True); exec(\"while True: pass\")'\n\x03echo after\n", { mark: 'looping' });
  assert.ok(line(out, 'after'), out);
});
