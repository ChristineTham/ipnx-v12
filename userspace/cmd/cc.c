/* cc: a C compiler driver for the demo toolchain. Compiles with clang,
 * links with wasm-ld against the wasi sysroot, and runs the result.
 * Relative sources resolve against the cwd (the shim now honours it).
 *   cc [file.c]      default: hello.c
 * The heavy guests (/bin/clang, /bin/wasm-ld) ride the toolchain boot
 * profile; without them cc says how to get them. */
#include "lib9.h"

#define nelem(x) (sizeof(x)/sizeof((x)[0]))

static char *src;
static char obj[] = "/tmp/_cc.o";
static char out[] = "/tmp/a.out";

static char *ccargv[] = {
	"clang", "-cc1", "-emit-obj", "-disable-free",
	"-isysroot", "/", "-internal-isystem", "/include/c++/v1",
	"-internal-isystem", "/include", "-internal-isystem", "/lib/clang/8.0.1/include",
	"-ferror-limit", "19", "-O2", "-o", obj, "-x", "c", nil, nil,
};
static char *ldargv[] = {
	"wasm-ld", "--no-threads", "--export-dynamic", "-z", "stack-size=1048576",
	"-L/lib/wasm32-wasi", "/lib/wasm32-wasi/crt1.o", obj, "-lc", "-o", out, nil,
};

static void ccchild(void *v) { USED(v); exec("/bin/clang", ccargv); exits("exec"); }
static void ldchild(void *v) { USED(v); exec("/bin/wasm-ld", ldargv); exits("exec"); }
static void runchild(void *v) { char *a[] = { out, nil }; USED(v); exec(out, a); exits("exec"); }

/* await one child; return 1 on clean exit (status '' between the quotes) */
static int
reap(void)
{
	char buf[128], *q;
	int n;

	n = await(buf, sizeof buf - 1);
	if(n <= 0) return 0;
	buf[n] = 0;
	q = strrchr(buf, '\'');		/* ...'status' -> last quote closes it */
	if(q == nil) return 0;
	*q = 0;
	q = strrchr(buf, '\'');		/* opening quote; empty between = success */
	return q != nil && q[1] == 0;
}

int
main(int argc, char *argv[])
{
	int fd;

	src = argc > 1 ? argv[1] : "hello.c";
	if(src[0] != '/'){			/* resolve against cwd so clang needn't guess */
		static char abs[512];
		int d = open(".", OREAD);
		if(d >= 0 && fd2path(d, abs, sizeof abs) >= 0){
			int l = strlen(abs);
			if(l == 1 && abs[0] == '/') l = 0;	/* root: avoid // */
			snprint(abs + l, sizeof abs - l, "/%s", src);
			src = abs;
		}
		if(d >= 0) close(d);
	}
	ccargv[nelem(ccargv) - 2] = src;

	if((fd = open("/bin/clang", OREAD)) < 0){
		fprint(2, "cc: the C toolchain is not loaded — boot the demo with the\n");
		fprint(2, "    \"C toolchain\" button (the plain desktop has no compiler).\n");
		exits("no toolchain");
	}
	close(fd);

	print("cc: compiling %s\n", src);
	procrfork(RFFDG, ccchild, nil);
	if(!reap()){ fprint(2, "cc: compile failed\n"); exits("compile"); }

	print("cc: linking %s\n", out);
	procrfork(RFFDG, ldchild, nil);
	if(!reap()){ fprint(2, "cc: link failed\n"); exits("link"); }

	procrfork(RFFDG, runchild, nil);
	reap();
	exits(nil);
	return 0;
}
