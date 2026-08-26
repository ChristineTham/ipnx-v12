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

static void
forkchild(void *v)
{
	char *av[] = { "forktest", nil };

	USED(v);
	exec("/bin/forktest", av);
	exits("exec");
}

static void
rcinteractive(void)
{
	char *av[] = { "rc", nil };

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

	/* The asyncify path: forktest is a transformed binary whose bare
	 * rfork(RFPROC) genuinely returns twice — its four checks print
	 * inline; its empty exit status is ours to verify. */
	pid = procrfork(RFFDG, forkchild, nil);
	n = await(buf, sizeof buf);
	ok(n > 0 && atoi(buf) == pid && strstr(buf, "''") != nil,
	   "asyncify: forktest suite ran clean");

	USED(pid);
	if(nfail == 0)
		print("poc: all %d tests passed\n", npass);
	else
		print("poc: %d passed, %d FAILED\n", npass, nfail);
	return nfail;
}
