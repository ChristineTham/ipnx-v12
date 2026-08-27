/* init: pid 1, and the PoC's test driver.
 * Proves: namespace assembly from a device, lazy fork with parent resume,
 * RFMEM sharing, per-process namespaces surviving exec. */
#include "lib9.h"

static int npass, nfail;
static void ok(int cond, char *what){
	if(cond){ npass++; print("PASS %s\n", what); }
	else    { nfail++; print("FAIL %s\n", what); }
}

int shared = 0;		/* data segment: visible to RFMEM children */

/* Children run inside the fork guard's extent (procrfork — RESEARCH §5.2):
 * mutate shared memory, rebind a namespace, then exec. */
static void
child1(void *v)
{
	char *av[] = { "cat", "/etc/motd", nil };

	USED(v);
	shared = 42;
	exec("/bin/cat", av);
	fprint(2, "init: exec /bin/cat: %r\n");
	exits("exec");
}

static void
child2(void *v)
{
	char *av[] = { "cat", "/etc/motd", nil };

	USED(v);
	bind("/lib/alt", "/etc", MREPL);
	exec("/bin/cat", av);
	exits("exec");
}

static void
rcchild(void *v)
{
	char *av[] = { "rc", "/rc/tests.rc", nil };

	USED(v);
	exec("/bin/rc", av);
	fprint(2, "init: exec /bin/rc: %r\n");
	exits("exec");
}

static int srvfds[2];

static void
srvchild(void *v)
{
	char *av[] = { "hellofs", nil };

	USED(v);
	dup(srvfds[1], 0);
	close(srvfds[0]);
	close(srvfds[1]);
	exec("/bin/hellofs", av);
	fprint(2, "init: exec /bin/hellofs: %r\n");
	exits("exec");
}

static void
catchild(void *v)
{
	char *av[] = { "cat", "/n/hello/motd", nil };

	USED(v);
	exec("/bin/cat", av);
	exits("exec");
}

static void
expchild(void *v)
{
	char *av[] = { "exportfs", nil };

	USED(v);
	/* arrange a private view first: this namespace copy, not the
	 * mounter's, is what travels over the wire */
	bind("/lib/alt", "/etc", MREPL);
	dup(srvfds[1], 0);
	close(srvfds[0]);
	close(srvfds[1]);
	exec("/bin/exportfs", av);
	exits("exec");
}

static void
expcat(void *v)
{
	char *av[] = { "echo", "ran", "across", "the", "export", nil };

	USED(v);
	exec("/n/exp/bin/echo", av);
	exits("exec");
}

static int idpipe[2];

static void
becomeuser(char *who)
{
	int fd;

	fd = open("/proc/self/ctl", OWRITE);
	if(fd < 0 || fprint(fd, "user %s", who) < 0)
		exits("ctl");
	close(fd);
}

static void
idchild(void *v)
{
	char *av[] = { "id", nil };

	USED(v);
	becomeuser("none");
	dup(idpipe[1], 1);
	close(idpipe[0]);
	close(idpipe[1]);
	exec("/bin/id", av);
	exits("exec");
}

static void
secretchild(void *v)
{
	USED(v);
	becomeuser("none");
	exits(open("/etc/secret", OREAD) < 0 ? nil : "opened the secret");
}

static void
climbchild(void *v)
{
	int fd;

	USED(v);
	becomeuser("none");
	fd = open("/proc/self/ctl", OWRITE);
	exits(fprint(fd, "user glenda") < 0 ? nil : "climbed back to glenda");
}

static void
setidchild(void *v)
{
	char *av[] = { "setid", nil };

	USED(v);
	becomeuser("none");
	dup(idpipe[1], 1);
	close(idpipe[0]);
	close(idpipe[1]);
	exec("/tmp/setid", av);
	exits("exec");
}

static void
dtestchild(void *v)
{
	char *av[] = { "dtest", nil };

	USED(v);
	exec("/bin/dtest", av);
	exits("exec");
}

static void
drtestchild(void *v)
{
	char *av[] = { "drtest", nil };

	USED(v);
	exec("/bin/drtest", av);
	exits("exec");
}

static void
forkchild(void *v)
{
	char *av[] = { "forktest", nil };

	USED(v);
	exec("/bin/forktest", av);
	exits("exec");
}

static void
threadtestchild(void *v)
{
	char *av[] = { "threadtest", nil };

	USED(v);
	exec("/bin/threadtest", av);
	exits("exec");
}

static void
samtestchild(void *v)
{
	char *av[] = { "samtest", nil };

	USED(v);
	exec("/bin/samtest", av);
	exits("exec");
}

static void
acmetestchild(void *v)
{
	char *av[] = { "acmetest", nil };

	USED(v);
	exec("/bin/acmetest", av);
	exits("exec");
}

/* ---- the WASI second ABI: foreign citizens through wasi1.mjs ---- */

static int wasipipe[2];

static void
wasichild(void *v)
{
	char *av[] = { "wasitest", "frominit", nil };

	USED(v);
	dup(wasipipe[1], 1);
	close(wasipipe[0]);
	close(wasipipe[1]);
	exec("/bin/wasitest", av);
	exits("exec");
}

static void
gochild(void *v)
{
	char *av[] = { "gotest", nil };

	USED(v);
	dup(wasipipe[1], 1);
	close(wasipipe[0]);
	close(wasipipe[1]);
	exec("/bin/gotest", av);
	exits("exec");
}

static void
pychild(void *v)
{
	char *av[] = { "python", "/tmp/pytest.py", nil };

	USED(v);
	dup(wasipipe[1], 1);
	close(wasipipe[0]);
	close(wasipipe[1]);
	exec("/bin/python", av);
	exits("exec");
}

/* drain a pipe until the writer closes */
static int
readall(int fd, char *buf, int cap)
{
	int n, t;

	t = 0;
	while(t < cap - 1 && (n = read(fd, buf + t, cap - 1 - t)) > 0)
		t += n;
	buf[t] = 0;
	return t;
}

/* ---- the kernel work for rc: rfork honesty and notes ---- */

static void
nomntchild(void *v)
{
	USED(v);
	if(bind("#c", "/tmp", MREPL) >= 0)
		exits("bind allowed");
	if(open("#c/pid", OREAD) >= 0)
		exits("hash walk allowed");
	exits("");
}

static void
gonechild(void *v)
{
	USED(v);
	exits("gone");
}

static void
seenchild(void *v)
{
	USED(v);
	exits("seen");
}

static void
notetestchild(void *v)
{
	char *av[] = { "notetest", nil };

	USED(v);
	exec("/bin/notetest", av);
	exits("exec");
}

static int notefds[2];

static void
catblocked(void *v)
{
	char *av[] = { "cat", nil };

	USED(v);
	dup(notefds[0], 0);
	close(notefds[0]);
	close(notefds[1]);
	exec("/bin/cat", av);
	exits("exec");
}

static void
rcinteractive(void)
{
	char *av[] = { "rc", "-i", nil };

	print("ipnx-v12: interactive rc (EOF to shut down)\n");
	exec("/bin/rc", av);
	fprint(2, "init: exec /bin/rc: %r\n");
	exits("exec");
}

int
main(int argc, char *argv[])
{
	char buf[256], name[64];
	uchar edir[512], *p;
	int fd, n, i, pid;
	volatile int canary;

	/* The namespace starts with only the root. Assemble /dev ourselves,
	 * the way a Plan 9 init does: bind the console device into place. */
	bind("#c", "/dev", MREPL);
	bind("#e", "/env", MREPL);
	bind("#d", "/fd", MREPL);
	bind("#s", "/srv", MREPL);
	fd = open("/dev/cons", ORDWR);
	dup(fd, 0);
	dup(fd, 1);
	dup(fd, 2);

	if(argc > 1 && strcmp(argv[1], "-i") == 0)
		rcinteractive();	/* replaces this image; rc's exit shuts down */

	print("ipnx-v12 poc: hosted kernel up, init is pid 1\n");
	ok(fd >= 0, "bind #c /dev and open /dev/cons");

	/* stat through the namespace */
	n = stat("/etc/motd", edir, sizeof edir);
	ok(n > 0 && statlen(edir) > 0 &&
	   strcmp(statname(edir, name, sizeof name), "motd") == 0,
	   "stat /etc/motd (9P2000 stat record)");

	/* Bare dual-return rfork(RFPROC) needs asyncify on a JS engine; the
	 * kernel must say so rather than wedge (RESEARCH §5.2). */
	ok(rfork(RFPROC) < 0, "bare rfork(RFPROC) refused with an error, not a wedge");

	/* Lazy fork: child runs in the guard's extent, writes shared memory,
	 * execs. Parent must resume at this call with the pid, stack intact. */
	canary = 0xC0FFEE;
	pid = procrfork(RFFDG, child1, nil);
	ok(pid > 0, "procrfork(RFFDG) returned pid into parent");
	ok(canary == 0xC0FFEE, "parent stack restored after child exec");
	ok(shared == 42, "RFMEM: child's write to data segment visible");
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid, "await reaped the child");

	/* Per-process namespace: the child gets a copy (RFNAMEG), rebinds
	 * /etc, execs cat — which must see the child's view. The parent's
	 * view must be untouched. */
	pid = procrfork(RFFDG|RFNAMEG, child2, nil);
	await(buf, sizeof buf);
	fd = open("/etc/motd", OREAD);
	n = read(fd, buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "hello") != nil,
	   "parent namespace untouched by child's bind over /etc");

	/* The shell: run the rc test script in a namespace copy; its binds
	 * must stay its own, and its exit status folds into ours. */
	pid = procrfork(RFFDG|RFNAMEG, rcchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "rc ran the test script and exited clean");
	fd = open("/etc/motd", OREAD);
	n = read(fd, buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "hello") != nil, "init's namespace survived rc's binds");

	/* Wire 9P at the mount boundary: a guest process serves 9P2000 on a
	 * pipe; mount(2) speaks Tversion/Tattach to it; every file operation
	 * below /n/hello is a wire message to that process. */
	pipe(srvfds);
	procrfork(RFFDG, srvchild, nil);
	close(srvfds[1]);
	n = mount(srvfds[0], -1, "/n/hello", MREPL, "");
	ok(n >= 0, "mount(fd): Tversion and Tattach negotiated with a guest server");
	close(srvfds[0]);			/* the kernel holds its own reference */

	fd = open("/n/hello/motd", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "wire 9P") != nil, "Twalk/Topen/Tread through the mount");

	n = stat("/n/hello/motd", edir, sizeof edir);
	ok(n > 0 && statlen(edir) > 0 &&
	   strcmp(statname(edir, name, sizeof name), "motd") == 0,
	   "Tstat through the mount");

	fd = open("/n/hello", OREAD);
	n = fd >= 0 ? read(fd, (char*)edir, sizeof edir) : -1;
	close(fd);
	i = 0;
	for(p = edir; p < edir+n; p += (p[0] | p[1]<<8) + 2)
		i++;
	ok(n > 0 && i == 2, "directory read over 9P: two integral stat records");

	fd = open("/n/hello/note", OWRITE);
	n = fd >= 0 ? write(fd, "hi over the wire", 16) : -1;
	close(fd);
	ok(n == 16, "Twrite through the mount");
	fd = open("/n/hello/note", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strcmp(buf, "hi over the wire") == 0, "written bytes read back over 9P");

	ok(open("/n/hello/nope", OREAD) < 0, "the server's Rerror arrives as errstr");

	pid = procrfork(RFFDG, catchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid, "a second process reads through the same mount");

	/* exportfs: a guest serving its own namespace back over 9P — the
	 * exporter privately rebinds /etc, and that private view is what
	 * the mounter sees. Then exec a binary THROUGH the export. */
	pipe(srvfds);
	procrfork(RFFDG|RFNAMEG, expchild, nil);
	close(srvfds[1]);
	n = mount(srvfds[0], -1, "/n/exp", MREPL, "");
	ok(n >= 0, "exportfs: mounted a guest exporting its namespace");
	close(srvfds[0]);
	fd = open("/n/exp/etc/motd", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "private view") != nil,
	   "exportfs: the exporter's private namespace is what travels");
	fd = open("/n/exp/lib", OREAD);
	n = fd >= 0 ? read(fd, (char*)edir, sizeof edir) : -1;
	close(fd);
	ok(n > 0 && strcmp(statname(edir, name, sizeof name), "alt") == 0,
	   "exportfs: directory reads relay across the export");
	pid = procrfork(RFFDG, expcat, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "''") != nil,
	   "exportfs: exec of a binary served over the export");

	/* The uid model (docs/uid.md): per-process credentials in the kernel,
	 * transitions through /proc/self/ctl, V10 enforcement in ramfs, and
	 * the DMSETUID bit at exec. The thing APE called impossible. */
	bind("#p", "/proc", MREPL);
	fd = open("/dev/user", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strcmp(buf, "glenda") == 0, "uid: /dev/user names the host owner");

	pipe(idpipe);
	pid = procrfork(RFFDG, idchild, nil);
	close(idpipe[1]);
	n = read(idpipe[0], buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(idpipe[0]);
	await(name, sizeof name);
	ok(strstr(buf, "none none") != nil,
	   "uid: setuid down via /proc/self/ctl, seen by an exec'd id");
	fd = open("/dev/user", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strcmp(buf, "glenda") == 0, "uid: the parent's credential is untouched");

	fd = create("/etc/secret", OWRITE, 0600);
	fprint(fd, "the host owner's business\n");
	close(fd);
	pid = procrfork(RFFDG, secretchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "uid: mode 0600 denies another user");
	fd = open("/etc/secret", OREAD);
	ok(fd >= 0, "uid: the owner still reads it");
	close(fd);

	pid = procrfork(RFFDG, climbchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "uid: a non-owner cannot climb back");

	/* the setuid bit: eve installs an id owned by tms with DMSETUID */
	fd = open("/bin/id", OREAD);
	i = create("/tmp/setid", OWRITE, 0755);
	while((n = read(fd, (char*)edir, sizeof edir)) > 0)
		write(i, edir, n);
	close(fd);
	close(i);
	ok(chown("/tmp/setid", "tms") == 0 && chmod("/tmp/setid", 0755|DMSETUID) == 0,
	   "uid: chown and chmod land as wstat (class B, no syscalls)");
	pipe(idpipe);
	pid = procrfork(RFFDG, setidchild, nil);
	close(idpipe[1]);
	n = read(idpipe[0], buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(idpipe[0]);
	await(name, sizeof name);
	ok(strstr(buf, "tms none") != nil,
	   "uid: DMSETUID elevates euid to the image's owner; ruid stays");

	/* Hard links and the symlink family — the V12 additions, and V10's
	 * resolution rule: a symlink resolves in the CLIENT's namespace. */
	fd = create("/tmp/orig", OWRITE, 0644);
	fprint(fd, "one file");
	close(fd);
	ok(link9("/tmp/orig", "/tmp/also") == 0, "link: a second name for a file");
	fd = open("/tmp/also", OWRITE);
	if(fd >= 0){ seek(fd, 0, 2); fprint(fd, ", two names"); close(fd); }
	fd = open("/tmp/orig", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strcmp(buf, "one file, two names") == 0,
	   "link: a write through either name lands in the one file");
	remove("/tmp/also");
	ok(open("/tmp/orig", OREAD) >= 0, "link: removing one name leaves the other");

	ok(symlink9("/etc/motd", "/tmp/lnk") == 0, "symlink: created");
	fd = open("/tmp/lnk", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "hello") != nil, "symlink: open follows to the target");
	n = readlink9("/tmp/lnk", buf, sizeof buf);
	ok(n > 0 && strcmp(buf, "/etc/motd") == 0, "readlink: the target comes back");
	n = lstat("/tmp/lnk", edir, sizeof edir);
	i = stat("/tmp/lnk", edir + 256, sizeof edir - 256);
	ok(n > 0 && i > 0 && statlen(edir) == 9 && statlen(edir + 256) > 9,
	   "lstat sees the link (9 bytes of target); stat sees the file");

	/* through the wire: create a symlink over the mount; the exporter's
	 * ramfs holds it, but READING it resolves in THIS namespace — the
	 * exporter's /etc is the alt view, ours is the real motd */
	ok(symlink9("/etc/motd", "/n/exp/tmp/wirelnk") == 0,
	   "symlink over the wire (minted Tsymlink)");
	fd = open("/n/exp/tmp/wirelnk", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "hello") != nil,
	   "a symlink read through a mount resolves in the CLIENT's namespace");
	ok(link9("/n/hello/motd", "/n/hello/motd2") < 0,
	   "a server without the extension answers Rerror, not a wedge");
	remove("/tmp/lnk");
	remove("/tmp/orig");

	/* The window server: dtest mints a window from #w, paints through its
	 * /dev/draw, and asserts pixels — headless, the same on every host. */
	pid = procrfork(RFFDG|RFNAMEG, dtestchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "wsys: dtest suite ran clean");

	/* The REAL libdraw: geninitdraw against a #w window, the default
	 * font included — rio's client library over our window server. */
	pid = procrfork(RFFDG|RFNAMEG, drtestchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "real libdraw: drtest suite ran clean");

	/* libthread, the wasm platform layer: coroutines over saved contexts,
	 * channels delivering through off-stack slots, and reads that park a
	 * thread while the process keeps scheduling (AREAD/IOWAIT). */
	pid = procrfork(RFFDG, threadtestchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "''") != nil,
	   "libthread: threadtest suite ran clean");

	/* The whole editor: sam over samterm in a window namespace — the
	 * mesg protocol, libframe, the font cache, libthread, async reads —
	 * typed at through wctl, verified in the raster. */
	pid = procrfork(RFFDG|RFNAMEG, samtestchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "sam+samterm: the editor round trip ran clean");

	/* And acme entire: its own 9P server mounted over a pipe, procexec,
	 * the mouse channel, columns and windows — driven by injected mouse
	 * chords, verified in the raster. */
	pid = procrfork(RFFDG|RFNAMEG, acmetestchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil, "acme: boots, serves, and opens a window by mouse");

	/* ---- the WASI second ABI: foreign citizens on the same kernel.
	 * wasitest is wasi-libc through the full sysroot; gotest is a REAL
	 * Go binary (GOOS=wasip1). Their one preopen is the namespace root —
	 * the citizenship clause, live. ---- */
	{
		static char out[2048];

		pipe(wasipipe);
		pid = procrfork(RFFDG, wasichild, nil);
		close(wasipipe[1]);
		readall(wasipipe[0], out, sizeof out);
		close(wasipipe[0]);
		n = await(buf, sizeof buf);
		ok(n > 0 && strstr(buf, "''") != nil &&
		   strstr(out, "hello from wasi-libc") != nil &&
		   strstr(out, "argv1=frominit") != nil &&
		   strstr(out, "clock=ticking") != nil &&
		   strstr(out, "motd: hello") != nil,
		   "WASI ABI: a wasi-libc citizen ran — stdio, argv, clock, the motd through the preopen");
		ok(strstr(out, "readback: written by wasi") != nil &&
		   strstr(out, "/etc has") != nil && strstr(out, "has 0 entries") == nil,
		   "WASI ABI: wrote, read back, and listed a directory through the namespace");

		pipe(wasipipe);
		pid = procrfork(RFFDG, gochild, nil);
		close(wasipipe[1]);
		readall(wasipipe[0], out, sizeof out);
		close(wasipipe[0]);
		n = await(buf, sizeof buf);
		ok(n > 0 && strstr(buf, "''") != nil &&
		   strstr(out, "hello from wasip1") != nil &&
		   strstr(out, "motd: hello") != nil,
		   "WASI ABI: a REAL Go binary (wasip1) ran against the kernel");
		ok(strstr(out, "slept=true") != nil &&
		   strstr(out, "/etc has") != nil && strstr(out, "has 0 entries") == nil &&
		   strstr(out, "readback: written by go") != nil,
		   "WASI ABI: Go slept on poll_oneoff, listed /etc, round-tripped a file");

		/* and REAL CPython — the second benchmark's interpreter, running
		 * a script file out of the namespace, its stdlib subset measured
		 * into the rootfs (wasi/pylib.txt) */
		pipe(wasipipe);
		pid = procrfork(RFFDG, pychild, nil);
		close(wasipipe[1]);
		readall(wasipipe[0], out, sizeof out);
		close(wasipipe[0]);
		n = await(buf, sizeof buf);
		ok(n > 0 && strstr(buf, "''") != nil &&
		   strstr(out, "script ran on wasi 3.14") != nil &&
		   strstr(out, "root: bin etc lib") != nil,
		   "WASI ABI: REAL CPython 3.14 booted and found its stdlib through the namespace");
		ok(strstr(out, "readback: written by python") != nil &&
		   strstr(out, "json: plan9 42") != nil &&
		   strstr(out, "sum: 4950") != nil,
		   "WASI ABI: Python imported json, round-tripped a file, computed");
	}

	/* The asyncify path: forktest is a transformed binary whose bare
	 * rfork(RFPROC) genuinely returns twice — its four checks print
	 * inline; its empty exit status is ours to verify. */
	pid = procrfork(RFFDG, forkchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "''") != nil,
	   "asyncify: forktest suite ran clean");

	/* ---- the kernel readied for rc: '..', unmount, honest rfork
	 * flags, and the note machinery (delivery at the syscall boundary,
	 * V7's timing — RESEARCH records the deviation) ---- */
	ok(chdir("/tmp") == 0 && open("../etc/motd", OREAD) >= 0,
	   "walk: '..' pops a component (cleanname's rule)");
	chdir("/");

	i = create("/tmp/a", OREAD, DMDIR|0755);
	if(i >= 0)
		close(i);
	i = create("/tmp/b", OREAD, DMDIR|0755);
	if(i >= 0)
		close(i);
	i = create("/tmp/a/onlya", OWRITE, 0644);
	if(i >= 0)
		close(i);
	i = create("/tmp/b/onlyb", OWRITE, 0644);
	if(i >= 0)
		close(i);
	bind("/tmp/a", "/n/u", MREPL);
	bind("/tmp/b", "/n/u", MAFTER);
	ok(open("/n/u/onlyb", OREAD) >= 0,
	   "union: the after-element answers for /n/u");
	ok(unmount("/tmp/b", "/n/u") == 0 && open("/n/u/onlyb", OREAD) < 0
	   && open("/n/u/onlya", OREAD) >= 0,
	   "unmount(name, old) removes one element; the rest stand");
	ok(unmount(nil, "/n/u") == 0 && open("/n/u/onlya", OREAD) < 0,
	   "unmount(nil, old) clears the mount point");

	pid = procrfork(RFFDG|RFNOMNT, nomntchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && strstr(buf, "''") != nil,
	   "RFNOMNT: bind and '#' walks refused in the child");

	pid = procrfork(RFFDG|RFNOWAIT, gonechild, nil);
	i = procrfork(RFFDG, seenchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == i && strstr(buf, "'seen'") != nil,
	   "RFNOWAIT: the abandoned child left no zombie");

	pid = procrfork(RFFDG, notetestchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "''") != nil,
	   "notes: alarm cancelled, blocked read interrupted, self-note handled");

	pipe(notefds);
	pid = procrfork(RFFDG, catblocked, nil);
	postnote(PNPROC, pid, "die");
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "'note: die'") != nil,
	   "an unhandled note kills; await reads it as the status");
	close(notefds[0]);
	close(notefds[1]);

	pipe(notefds);
	pid = procrfork(RFFDG|RFNOTEG, catblocked, nil);
	postnote(PNGROUP, pid, "stop");
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "'note: stop'") != nil,
	   "notepg reaches the child's group; RFNOTEG kept init out of it");
	close(notefds[0]);
	close(notefds[1]);

	USED(pid);
	if(nfail == 0)
		print("poc: all %d tests passed\n", npass);
	else
		print("poc: %d passed, %d FAILED\n", npass, nfail);
	return nfail;
}
