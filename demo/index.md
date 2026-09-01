# Saranos

*a modern operating system for a simpler life*

**Saranos** is an operating system small enough to run in a browser, but powerful enough to span from the cloud to your personal device.

It is a refuge from the complexities of the current computing landscape:

- pods, containers, hypervisors, virtual machines, jails, services
- Kubernetes, Linux, systemd, Docker, Compose
- package managers (npm, pip, cargo, apt), version managers (nvm, pyenv, rbenv)
- CI/CD pipelines, microservices architecture, API gateways
- configuration management (Ansible, Terraform, CloudFormation), orchestration layers
- observability stacks (Prometheus, Grafana, ELK, Jaeger), monitoring overhead
- multiple runtimes per machine, dependency hell, the Java heapocalypse

**Saranos** will enable you to do the equivalent of most of the above, but in a simpler integrated way, on the devices and the interfaces you are already used to. It can run completely in the browser, on top of any operating system, but it also installs as an app on the Mac, iPad or iPhone. It will also install as a container, and there are plans to make it available as a micro-VM to run inside a hypervisor, or natively on an IoT device. In short, it can run on Everything, Everywhere, All at Once.

**Saranos** is a reimagining of UNIX if it was implemented today, combined with a post-modern windowing system and user interface. It embraces the UNIX philosophy:

> Write programs that do one thing and do it well. Write programs to work together. Write programs to handle text streams, because that is a universal interface”
>
> (McIlroy et al., UNIX time-sharing system: Foreword, 1978)

It also adheres to the principles set by the designers of *Plan 9 from Bell Labs*, the successor to UNIX from it's original creators:

> First, resources are named and accessed like files in a hierarchical file system. Second, there is a standard protocol, called 9P, for accessing these resources. Third, the disjoint hierarchies provided by different services are joined together into a single private hierarchical file name space.
>
> (Pike et al., Plan 9 from Bell Labs)

**Saranos** is built from another project called [IPNX](https://github.com/ChristineTham/ipnx) - a MacOS/iPadOS/iOS revitalisation of Research Unix 8th and 10th editions, running on a simulator on macOS/iOS.

The underlying kernel and userspace of Saranos is IPNX v12 
**Saranos is a new operating system** — not a distribution of an old one, not an emulator. Three layers, and the parallel is exact: **Saranos** is the system, **IPNX** is the kernel and the userspace, **emca** is the windowing and UI — where Apple has macOS, Darwin and Aqua. The name is Sanskrit *śaraṇa*, refuge: a refuge from the complexities of the modern computing environment.

Its kernel is original, written in Rust — and **the very same kernel runs native on macOS and, compiled to WebAssembly, inside this tab**: one core, one conformance suite green on both, plus a JavaScript twin held as the frozen conformance oracle. The architecture takes what Plan 9 proved and Unix never shipped — **per-process namespaces**, 9P as the only IPC, everything a file — with none of Plan 9's kernel code, and **WebAssembly** as the executable format.

What boots below is the complete system, entirely in this tab: it boots into **emca**, which is not an application but the system's face — one object, a window, composited recursively into rows, columns and tabs, where every window is itself a compositor. With `con`, `acme` and `sam` (each name inherited by passing its ancestor's tests; the raster originals still run as `sam9` and `acme9`), the real 4th-edition `rc`, twenty-four real commands, V10 binaries from the TUHS tapes in `/v10/bin`, real Go and CPython 3.14, a C compiler, a package manager, whole-system snapshots and a process orchestrator. Nothing you type leaves the page. **No other wasm runtime, development environment or Linux distribution can easily claim this**: a whole operating system — kernel, editors, compilers, Python with pip — running securely in a browser tab, in an internet café if need be; and the same system runs native on macOS with full JIT.

=> [**Boot Saranos** — the whole world: emca with your home listed, files in editor tabs and `rc` below; shells, sam, acme, cc, the real Go compiler, Python with pip. It appears in seconds; the toolchains (~260 MB) stream in while you look around. Nothing persists by default.](shell/)

=> [Run the conformance suite](browser/)

## What's inside

**acme** — the one editor, acme reimagined for modern surfaces and answering the 1994 paper example for example: columns of windows, every tag an editable line whose words are the commands, a directory listing as the file browser, sam's structural regexps in every tag (`Edit ,x/re/c/text/`), the dirty box (and `Put` appears in the tag only while it's needed). Execute any command in a tag — its output opens in a `dir/+Errors` window; `|`, `<` and `>` filter the selection through real programs; `Undo`/`Redo` unwind by sequence; right-click `hello.c:3` and the file opens *at line three*. And the editor is itself files: `mount acme /mnt/acme`, then `grep -n main *.c > /mnt/acme/new/body` puts grep's output in a fresh window — the paper's own example, running. **sam** — the same language as a standalone filter: `sam file < commands`, ed's true heir; it earned the name by passing the original's tests. **con** — the console as an editable transcript: scrollback is a buffer, search is the editor, no terminal emulation anywhere in the native path.

**pkg** — the package manager where *installing is a bind*: a package is a verified subtree, `pkg install ruby` binds its binaries into `/bin`, versions coexist, conflicts are refused at install time, and a subshell that does `rfork n` owns a private dev environment that vanishes with it — no venv, no nvm, no flatpak, because the kernel can say `bind`.

**#V** — every system is a time machine: `echo snap t1 > '#V/ctl'` freezes the filesystem by copy-on-write (nine megabytes for twenty whole-system snapshots), `cat '#V/t1/tmp/f'` reads the past, and rollback is `bind '#V/t1/dir' /dir`. Nobody rewrites history — not even the owner.

**run & svc** — containers and orchestration without the industry: a process spec is a directory (a Dockerfile is a script because installing is mutation; here it's a declaration because installing is a bind); `svc` keeps N replicas alive and kubectl is `cat` and `echo`: `echo start web /spec 3 > /n/svc/ctl`.

**Real toolchains** — `cc` is clang and wasm-ld running as guests; `go` drives the real gc compiler; `python` is CPython 3.14 with the full stdlib and `pip` talking to the real PyPI. And the display itself is files: every window's UI can be `cat`-ed, `echo`-ed into, and `grep`-ed — which is why the test suite drives real editors without a screen.

## emca, the interface

Press **Boot** and the whole page is emca. There is **one object**: a window is a rectangle with a tag, holding either a body or child windows — so a column is a window too, and closing one closes what it holds. Every window composites itself, without limit, which is why *panes* are not a kind of thing here: the root window makes three columns by convention and then forgets they were special.

A parent gives rectangles to some of its children; the rest are **tabs**. That single idea is **minimise** (move me out of the allocation), **maximise** (move everyone else out) and **new tab** (the same again with the allocation bit flipped) — one mechanism, three buttons, and a tab is a whole window that simply has no rectangle.

Every window carries the same four parts: coloured controls and an **editable title** — retitle a window and it retargets, which is how you navigate — then the **tag line** with its own verbs, then the body with scrollbars, then a status line. The tag line is an *operand*, not a command line: type `mk` and press **Run**; type `alice` and press **Find**, which selects *every* match, so replacing is just typing. Leave it empty and the verbs act on the selection instead. Output goes to `/output/<dir>/<command>` — a real path in a filesystem that mirrors the filesystem, so it `cat`s and `grep`s like anything else.

The display really is files. From the shell below, try `ls /dev/window`, `cat /dev/window/edit/1/toolbar`, or `rc /rc/tile` to run a window manager written in a dozen lines of shell. Your ⌘C/⌘X/⌘V are snarf: `cat /dev/snarf` reads what you last copied. And the raster heritage is one command away — `win acme9 &` opens the 1993 acme, verbatim source, beside its successor.

## The macOS surface

The same system runs native — full JIT under wasmtime, real windows, your real pasteboard, and host storage. With [Rust](https://www.rust-lang.org/tools/install), wasi-sdk, binaryen, `bison` and `go` installed (versions and paths in `docs/handbook.md`):

```
git clone https://github.com/ChristineTham/ipnx-v12 && cd ipnx-v12
bash userspace/mk.sh                                      # build the guest world
cargo run --release -p host -- userspace/rootfs           # headless: init runs the suite
cargo run --release -p host -- userspace/rootfs --app -i  # windows: con, acme, canvas
cargo run --release -p host -- userspace/rootfs --live    # writes persist to the rootfs dir
bash hosts/macos/mkapp.sh                                 # wrap it as IPNX.app
```

Under `--app` the canvas renders natively; `/dev/snarf` *is* the Mac pasteboard (`echo hi > /dev/snarf` then paste anywhere); and the versioning layer runs everywhere: `echo snap t1 > '#V/ctl'` freezes the filesystem, `bind '#V/t1/dir' /dir` is the rollback — twenty whole-system snapshots cost nine megabytes.

Once the prompt appears, take the guided tour:

```
rc /rc/tour
```

You boot as **kitty**, at home in `/usr/kitty`, where `hello.c`, `hello.py` and `hello.go` are waiting. With the toolchain aboard, `cc hello.c` then `./a.out` runs real clang and wasm-ld as guests, then the binary you built — Hello Kitty. It is a real `cc(1)`: flags, `-o`, `-c` and multiple files all work. Python interprets in the tab too (`python hello.py`); real Go binaries run, though Go's compiler is host-side (`go` explains why). The tour shows everything.

Heritage is one command away: `font=/lib/font/bit/go/regular.13.font win acme9 &` opens the 1993 raster acme, verbatim source, beside its successor; `@{bind /lib/alt /etc; cat /etc/motd}` shows a subshell rearranging its own private namespace; `/v10/bin/echo -e 'a\nb' | wc` pipes 1989 into 1992.

> Everything runs in your browser — nothing you type leaves this tab, and reloading forgets it all unless you chose a persistent home. **Measured green in Chromium (Chrome, Edge, Brave, Arc) and in Safari** — the full conformance suite (158 tests today; the floor is 131) passes in both. (Safari needed real engineering: WebKit deterministically fails concurrent module-worker loads through a service worker — minimal repro and file-ready report in the repository under `demo/webkit-repro/` — so this page serializes worker startup.) Firefox is untested. The whole system — kernel, sources, the conformance suite, and the documents that argue for it — is at [github.com/ChristineTham/ipnx-v12](https://github.com/ChristineTham/ipnx-v12). Third-party licences: [NOTICES](NOTICES.html).
