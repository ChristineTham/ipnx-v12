# The proof of concept — the record (2026-08-26 → 2026-08-29, complete)

**Declared complete 2026-08-29.** The PoC ran three days by the calendar and one
architecture by intent: the full design of [v12-plan.md](v12-plan.md), built as a
working slice until it had nothing left to prove. Final state: **131 acceptance
tests, green on three hosts** — the JS reference kernel on Node, the same kernel
in Chrome, and the Rust kernel core under wasmtime — with the real Plan 9
userspace (rc, sam, acme, twenty-four commands, the libraries under them), the
V10 exhibit, and three foreign citizens (wasi-libc, Go `wasip1`, CPython 3.14)
running on it.

**What the PoC is now: the reference implementation and the conformance oracle.**
[poc/](../poc/) is frozen — the JS supervisor changes no more. Its value from
here is that it *runs*: `bash poc/run.sh` must print 131 PASS lines on any
future day, because those 131 tests are the floor every new host must reach
before it is real (the conformance policy is in
[implementation.md](implementation.md)). The guest world currently inside
`poc/` — the vendored userspaces, the libcs, the rootfs, `mk.sh` — is **not**
frozen: it is the real userspace, shared by every host, and graduates out of
`poc/` as implementation milestone M0.

What the PoC deliberately did not do is listed in
[poc/README.md](../poc/README.md); the findings it produced are in
[RESEARCH.md](../RESEARCH.md) (§5 fork/transport, §7 GUI, §9.4–9.6 toolchain,
native core); the decisions it settled are dated entries in the
[decision log](v12-plan.md).

---

The chronology below is the PoC section of v12-plan.md as it accreted, milestone
by milestone, moved here verbatim on the day of declaration — a period record,
including its forward-looking sentences, each of which came true.

## The proof of concept (2026-08-26)

[poc/](../poc/) is the architecture's working slice: the supervisor in Node, guests in
freestanding C compiled with wasi-sdk's clang (RESEARCH §9.4), one Worker per process,
per-process namespaces, Plan 9 trap numbers, 9P2000 stat records — booting to **the
real 4th-edition `rc`** over a writable ramfs and twenty-four real commands, and
**wire 9P at a mount boundary**: `hellofs`, a 9P2000
server in a guest process serving on a pipe, is attached with `mount(2)`
(Tversion/Tattach), and every operation below the mount point is one tagged wire message
through the mount driver — the only place the kernel marshals 9P, exactly the
Dev-table-inside decision — and **the asyncify path**: bare dual-return `rfork(RFPROC)`
on transformed binaries (RESEARCH §5.2), which is what runs every fork the real rc
makes — pipelines, subshells, captures — as forked copies of the interpreter. 102
acceptance tests pass
end-to-end: kernel, mount and fork tests (lazy-fork resume, namespace isolation through
`exec`, Twalk/Topen/Tread/Tstat/Twrite over the wire, integral directory reads, `Rerror`
arriving as `errstr`, two processes sharing one connection, dual return with copied
memory and intact parent locals) and seventeen shell tests (pipelines, `` `{...} ``
substitution, glob, redirections, `$status`, `for`/`if`, `bind` as an ordinary command,
subshells as pipeline stages with copy semantics and status propagation).

```sh
bash poc/mk.sh && bash poc/run.sh     # the tests
bash poc/run.sh -i                    # boot to an interactive rc
```

Union directories complete the namespace algebra — `bind -a`/`-b`/`-c`, ordered walks,
concatenated directory reads, creates landing in the MCREATE element — and **exportfs**
completes the boundary: a guest serving its own namespace (private binds included) over
wire 9P, from which another process can read, list, and **exec binaries**. Both
directions of the mount boundary are now guest-reachable.

The kernel is **one platform-neutral module with two hosts**: `bash poc/run.sh` boots it
on Node, and `node poc/serve.mjs` serves the same kernel into a page (the server exists
only to set the COOP/COEP headers SharedArrayBuffer requires) — the full suite passes in
Chrome 148, and `?i` boots the page to an interactive rc in a console window
(RESEARCH §5.3, §7 for the Wanix/Apptron precedent that shapes where the GUI goes next).

What it deliberately does not do is listed in [poc/README.md](../poc/README.md). The
**The userspace objective, stated once**: real Plan 9 userspace and real Research Unix
V10 userspace, compiled to wasm from their own trees, running side by side on this
kernel — each against its own libc.a rewritten over the kernel interface (`lib9` for
Plan 9, `libv10` for V10; both this project's code, in poc/). The first citizens of each
are in: 4th-edition `cat` and `echo` compiled **unmodified** through shim headers and now
doing all the suite's work, and TUHS-tape V10 `cat` and `echo` — K&R C, `-std=c89
-fno-builtin`, implicit declarations left authentic — living in `/v10/bin`, fingerprinted
by `echo -e`, and piping into Plan 9 `cat` in a single pipeline. Growing both userspaces
command by command is now the PoC's standing work — and the first sweep landed: the
REAL `/sys/include/libc.h` over one platform shim, the real libc (port/fmt/9sys),
libbio, libregexp and libString as `libp9.a`, twenty-four real commands including
`ls`, `sed`, `grep` and `sort` — and **the real `rc`** (bison over `syn.y`,
asyncified), running the whole suite interactively, in batch, and in a window, over a
kernel readied for it: notes delivered at the syscall boundary, `alarm`, `unmount`,
honest rfork flags (`RFNOMNT`/`RFCNAMEG`/`RFCFDG`/`RFNOWAIT`/`RFNOTEG`), the dup
device `#d`, and `..` in walks. V10 growth waits, per direction, for the parent
project's ANSI conversion of its userspace. Platform order ahead: **macOS native first,
then iPadOS**, as a **Rust kernel core plus per-platform embedding shims** (decision
below, 2026-08-27). The engineering lifts the plan named are done, the uid model is
designed ([docs/uid.md](uid.md)) and running, the window server speaks the real draw
device protocol (screens, window views, clipping, channel-correct uploads,
`b d f L e E y i l s x c A F t O v`), and **the whole editor runs**: the real `sam`
over the real `samterm`, libframe over libdraw over the wasm libthread, typed at in a
browser window and headlessly under samtest. And **the PoC is closed** (2026-08-27):
**the real `acme`** — the GUI decision's declared real test — boots in a browser
window and under acmetest, all twenty of its source files verbatim through the
derivation layer (the load-bearing find: kencc adjusts pointers to unnamed
substructures at call sites and clang does not — RESEARCH §9.5's `frameadjust.h`
shape), its own 9P file server armed over a pipe, the mouse crossing wctl with
button-2 execute and button-3 look verified in the raster, the float door opened
(`strtod`/`fltfmt` verbatim over the real `FPdbleword`), and the kernel grown its
last two PoC devices: `#s` (srv — a posted fd's channel kept alive by name, which
is what makes acme's error pipe park instead of EOF-spinning) and `#d` bound at
`/fd`. **124 acceptance tests, green on Node and in Chrome.** The PoC has nothing
left to prove; the native work begins. And the post-PoC queue's first item is
already moving (same day): **the WASI second ABI runs** — `wasi1.mjs`
implements `wasi_snapshot_preview1` over the same mailbox with fd 3, the one
preopen, as the namespace root; a wasi-libc citizen and a **real Go binary**
(`GOOS=wasip1`, go1.25.6) read the motd, list directories, round-trip files
and sleep on `poll_oneoff`, on both hosts — and **REAL CPython 3.14.7** (the
wasi build, 30.5MB) boots by landmark, imports its stdlib from a 21-file
measured subset (`wasi/pylib.txt` — everything else is frozen into the
binary), runs a script out of the namespace and round-trips json — **130
tests**. Two of the three benchmark runtimes speak to the kernel. **git is
deferred, deliberately (2026-08-27)**: porting it to wasi now would measure
the wrong surface — git is the modern personality's benchmark, so it gets
*built under that personality* once `libunix` exists (compilation as a
capability, per the `/cc` decision), not hand-carried around it. The Rust
kernel core is under way (same day): `native/` holds the kernel crate (the
core as a pure state machine — syscalls in, effects out, the per-platform
seam) and the macOS host shim (wasmtime 37, a thread per guest, the guard as
a plain host function — RESEARCH §9.6). **96 of the 130 conformance tests
pass**: the real rc's whole script, sam -d, forktest, links, unions, wstat,
unmount, the note machinery, the uid suite via devproc, libthread on native
AREAD/IOWAIT — and, on the async core (the kernel's own 160-line executor,
no tokio, effect seam unchanged), **wire 9P entire**: mount(fd), hellofs,
exportfs with private namespaces travelling, symlinks over minted message
types, wstat through the wire. The WASI shim
ported to wasmtime host functions and passed all six citizen tests on its
first run. The window server and draw engine followed, and **the suite
closed: 130 of 130 on all three hosts — Node, Chrome, and the Rust kernel
under wasmtime.** The native milestone the decision log defined is
delivered: the conformance spec passes identically on the reference
implementation and the rewrite. What the Rust milestone was recorded to
unlock comes next: the `FROM scratch` OCI container (stated as free with
this milestone), then the iPadOS shim on Pulley per the engine matrix.

## The last day (2026-08-29): identity, and the declaration

The final additions before the declaration were not kernel mechanism but
meaning: the five identity decisions (su as transition never escalation; the
user decomposed into person, role, agent, and network person; the profile as a
file tree; the capability doctrine from the graveyard — all dated 2026-08-29 in
the decision log, told as one story in [uid.md](uid.md) and RESEARCH §12), and
their one running artifact: `su` ([poc/cmd/su.c](../poc/cmd/su.c)), the
privilege-*drop* shell, whose test made the suite **131** — the number the
declaration freezes. The boot namespace gained `#p` at `/proc` so identity is
visible from the first process on.

## What the PoC proved, in one list

- The hosted-kernel architecture **works**: one kernel design, three hosts
  (Node, Chrome, wasmtime/macOS), identical conformance.
- The compatibility thesis **works**: unmodified Plan 9 sources — rc, sam,
  samterm, acme, the libraries — and unmodified foreign binaries (Go, CPython)
  run on the same kernel through two ABIs (Plan 9 traps; WASI preview1).
- The namespace is a sufficient security model at PoC scale: uid model, per-attach
  identity, V10 enforcement, `su none` — with no superuser anywhere.
- 9P at the boundary and a Dev table inside compose: wire mounts, exportfs both
  directions, symlinks over minted types, all under the same walk.
- The platform costs are known and paid: asyncify's +103% on rc, the fork
  guard's shape on both hosts, the SAB TextDecoder rule, kencc's call-site
  adjustment — every one measured and recorded in RESEARCH.
