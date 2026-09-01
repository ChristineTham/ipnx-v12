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

The name **Saranos** is derived from Sanskrit *śaraṇa* (refuge): a refuge from the complexities of the modern computing environment.

**Saranos** is a reimagining of UNIX if it was implemented today, combined with a post-modern windowing system and user interface. It embraces the UNIX philosophy:

> Write programs that do one thing and do it well. Write programs to work together. Write programs to handle text streams, because that is a universal interface
>
> (McIlroy et al., UNIX time-sharing system: Foreword, 1978)

It also adheres to the principles set by the designers of *Plan 9 from Bell Labs*, the successor to UNIX from its original creators:

> First, resources are named and accessed like files in a hierarchical file system. Second, there is a standard protocol, called 9P, for accessing these resources. Third, the disjoint hierarchies provided by different services are joined together into a single private hierarchical file name space.
>
> (Pike et al., Plan 9 from Bell Labs)

**Saranos** is built from another project called [IPNX](https://github.com/ChristineTham/ipnx) - a macOS/iPadOS/iOS revitalisation of Research Unix 8th and 10th editions, running on a simulator on macOS/iOS.

The underlying kernel and userspace of Saranos is IPNX v12 - with a reimplementation of the Plan 9 kernel on Rust, compiled to WASM, but with a Unix v10 personality. The userspace supports a combination of Plan 9 and Unix v10 utilities, plus modern language toolchains and packages such as Rust, Python and Go — Python and Go run today — and it fully interoperates with WASI binaries.

The windowing system and user interface is an evolution of *acme*, Bell Labs' "user interface for programmers". We generalise the concepts behind acme into a user interface that we call **emca** ("acme" reversed).

**Saranos** is intended to be a distributed operating system (aspirational vision, not reality). It can orchestrate across multiple instances of itself, all owned by you, from the cloud to personal devices.

So in short
- **Saranos** an operating system that can run on Everything, Everywhere, All at Once
- using a user interface and windowing system called **emca** ("acme" reversed) supporting the browser and Apple devices
- on a kernel called IPNX modelled after *Plan 9* but written in Rust and compiled into WASM
- supporting WASM binaries from multiple personalities (Plan 9, UNIX, WASI, etc.)

It has functionality similar to but not supplanting:

- pod and container orchestration (IPNX processes have per process namespaces and run isolated from each other, and can coexist together)
- package management and dependencies (IPNX packages are simply mappings into namespaces)
- distributed computing (all processes communicate via a single protocol called 9P)

What boots below is the whole system, in this tab. Nothing installs, nothing you type leaves the page, and a reload forgets it existed.

=> [**Boot Saranos** — your home listed on the left, files open in tabs, `rc` running below. It appears in seconds; the C, Go and Python toolchains (~260 MB) stream in behind you while you look around.](shell/)

=> [Run the conformance suite](browser/)

## What you get

**emca** — the interface, and the whole page is it. There is no desktop behind it and no application in front of it.

A **window** is a rectangle with a tag, holding either a body or other windows. A column is a window too, which is why closing one closes what it holds, and why every window can divide itself again without limit. What you will see at boot is your home directory listed on the left, `/etc/motd`, `/rc/tour` and your `README` open as tabs, and `rc` in the row below — every one of them an ordinary window.

Each carries the same parts: a title you can edit, a row of the verbs its **type** allows, a tag line where a command runs in that window's own directory, and a body. The type comes from `/type`, which is itself a directory of small files — so adding a manager to the system is adding a file, not writing a program. That is why the buttons along the top open `/proc`, `/pkg`, `/usr` and `/type` as ordinary windows and no process-manager exists anywhere.

The display really is files. From the shell, `ls /dev/window` lists the windows, `cat /dev/window/text/2/toolbar` reads one's verbs, and `rc /rc/tile` runs a window manager written in a dozen lines of shell. Your ⌘C/⌘X/⌘V are snarf: `cat /dev/snarf` reads what you last copied.

**acme**, **sam** and **con** run here too — each inherited its name by passing its ancestor's tests, and the 1993 raster originals are still one command away as `acme9` and `sam9`. acme keeps its own vocabulary: `Put`, `Get`, `Snarf`, `Zerox`, the `|` `<` `>` filters, and `mount acme /mnt/acme` so the editor is itself files.

Underneath them is the real 4th-edition `rc` and twenty-four real Plan 9 commands, with V10 binaries from the TUHS tapes in `/v10/bin` — `/v10/bin/echo -e 'a\nb' | wc` pipes 1989 into 1992.

**pkg** — installing is a **bind**. A package is a verified subtree; `pkg install ruby` binds its binaries into `/bin`; versions coexist; conflicts are refused at install time; and a subshell that does `rfork n` owns a private environment that vanishes with it. No venv, no nvm, no flatpak, because the kernel can say `bind`.

**#V** — every system is a time machine. `echo snap t1 > '#V/ctl'` freezes the filesystem by copy-on-write — nine megabytes for twenty whole-system snapshots — `cat '#V/t1/tmp/f'` reads the past, and rollback is `bind '#V/t1/dir' /dir`. Nobody rewrites history, not even the owner.

**run & svc** — orchestration without the industry. A process spec is a directory: a Dockerfile has to be a script because installing is mutation, and here it is a declaration because installing is a bind. `svc` keeps N replicas alive, and kubectl is `cat` and `echo`: `echo start web /spec 3 > /n/svc/ctl`.

**Real toolchains, running now** — `cc hello.c` then `./a.out` is real clang and real wasm-ld, as guests. `go run hello.go` drives the real gc compiler and linker, also guests, because they are pure Go and cross-build. `python hello.py` is CPython 3.14 with the full standard library, and `pip install cowsay` talks to the real PyPI. Try them in the shell below once the toolchains have streamed in — `examples/` in your home has runnable programs for each.

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

You boot as **kitty**, at home in `/usr/kitty`, where `hello.c`, `hello.py` and `hello.go` are waiting. With the toolchain aboard, `cc hello.c` then `./a.out` runs real clang and wasm-ld as guests, then the binary you built — Hello Kitty. It is a real `cc(1)`: flags, `-o`, `-c` and multiple files all work. Python interprets in the tab too (`python hello.py`); the real gc compiler and linker run as guests too, because they are pure Go and cross-build (`go` explains how). The tour shows everything.

Heritage is one command away: `font=/lib/font/bit/go/regular.13.font win acme9 &` opens the 1993 raster acme, verbatim source, beside its successor; `@{bind /lib/alt /etc; cat /etc/motd}` shows a subshell rearranging its own private namespace; `/v10/bin/echo -e 'a\nb' | wc` pipes 1989 into 1992.

---

Everything runs in your browser — nothing you type leaves this tab, and reloading forgets it all unless you chose a persistent home. **Measured green in Chromium (Chrome, Edge, Brave, Arc) and in Safari** — the full conformance suite (160 tests today; the floor is 131) passes in both. (Safari needed real engineering: WebKit deterministically fails concurrent module-worker loads through a service worker — minimal repro and file-ready report in the repository under `demo/webkit-repro/` — so this page serializes worker startup.) Firefox is untested. The whole system — kernel, sources, the conformance suite, and the documents that argue for it — is at [github.com/ChristineTham/ipnx-v12](https://github.com/ChristineTham/ipnx-v12). Third-party licences: [NOTICES](NOTICES.html).
