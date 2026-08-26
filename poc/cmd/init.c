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
	uchar edir[512];
	int fd, n, pid;
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

	USED(pid);
	if(nfail == 0)
		print("poc: all %d tests passed\n", npass);
	else
		print("poc: %d passed, %d FAILED\n", npass, nfail);
	return nfail;
}
