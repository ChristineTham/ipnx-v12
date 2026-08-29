/* cc: a real C compiler driver for ipnx.
 *
 * WASI has no way to spawn a subprocess, so clang's own driver cannot run
 * here (nor can `go build` — same wall). ipnx does have fork+exec, so this
 * driver — an ordinary ipnx process — orchestrates clang's pieces the way
 * clang's driver would on a normal Unix: it runs `clang -cc1` to compile each
 * source, then `wasm-ld` to link. What binji's wasm-clang does in JavaScript,
 * ipnx does with a real process model.
 *
 * Behaves like cc(1): silent on success; `cc f.c` makes a.out; -o names the
 * output; -c stops at the object; -O/-I/-D/-W/-std/-f/-g and friends pass
 * through to the compiler; -l/-L pass through to the linker; multiple sources
 * and .o inputs work. It does NOT run the program — run ./a.out yourself.
 */
#include "lib9.h"

#define nelem(x) (sizeof(x)/sizeof((x)[0]))
enum { MAXARG = 256 };

/* the fixed clang -cc1 preamble: freestanding target, the wasi sysroot */
static char *ccpre[] = {
	"clang", "-cc1", "-triple", "wasm32-unknown-wasi", "-emit-obj",
	"-disable-free", "-isysroot", "/",
	"-internal-isystem", "/include/c++/v1",
	"-internal-isystem", "/include",
	"-internal-isystem", "/lib/clang/8.0.1/include",
	"-ferror-limit", "19",
};
static char *ldpre[] = {
	"wasm-ld", "--no-threads", "-z", "stack-size=1048576",
	"-L/lib/wasm32-wasi", "/lib/wasm32-wasi/crt1.o",
};

static char *cflags[MAXARG]; static int ncf;	/* extra compile flags */
static char *lflags[MAXARG]; static int nlf;	/* extra link flags */
static char *srcs[MAXARG];   static int nsrc;	/* .c sources */
static char *objs[MAXARG];   static int nobj;	/* .o inputs + produced */
static char *out;				/* -o */
static int compileonly;				/* -c */
static char *stopat;				/* -E or -S: stop before objects */

static char *
tosuf(char *s, char *dir, char *suf)		/* foo.c -> <dir>foo<suf> */
{
	char base[256], *p, *q;

	q = strrchr(s, '/');
	strncpy(base, q ? q + 1 : s, sizeof base - 1); base[sizeof base - 1] = 0;
	p = strrchr(base, '.');
	if(p) *p = 0;
	return smprint("%s%s%s", dir, base, suf);
}

static char *
absolutize(char *s)				/* clang shouldn't guess the cwd */
{
	static char cwd[512];
	int d, l;

	if(s[0] == '/') return s;
	d = open(".", OREAD);
	if(d < 0 || fd2path(d, cwd, sizeof cwd) < 0){ if(d>=0) close(d); return s; }
	close(d);
	l = strlen(cwd);
	if(l == 1 && cwd[0] == '/') l = 0;
	return smprint("%s/%s", cwd, s);
}

static void child(void *v){ char **b = v; exec(smprint("/bin/%s", b[0]), b); exits("exec"); }

/* fork b, wait, return 1 on clean exit (empty '' status) */
static int
run(char **b)
{
	char buf[128], *q;
	int n;

	procrfork(RFFDG, child, b);
	n = await(buf, sizeof buf - 1);
	if(n <= 0) return 0;
	buf[n] = 0;
	q = strrchr(buf, '\'');
	if(q == nil) return 0;
	*q = 0;
	q = strrchr(buf, '\'');
	return q != nil && q[1] == 0;
}

int
main(int argc, char *argv[])
{
	char *a[MAXARG]; int na, i, j, fd;

	for(i = 1; i < argc; i++){
		char *s = argv[i];
		int len = strlen(s);

		if(strcmp(s, "-c") == 0) compileonly = 1;
		else if(strcmp(s, "-E") == 0 || strcmp(s, "-S") == 0) stopat = s;
		else if(strcmp(s, "-o") == 0 && i+1 < argc) out = argv[++i];
		else if(strncmp(s, "-o", 2) == 0 && len > 2) out = s+2;
		else if(strcmp(s, "-lm") == 0)
			{}					/* wasi-libc folds libm into libc */
		else if(strncmp(s, "-l", 2) == 0 && len > 2){
			/* -lX: an archive passes through; a package that ships bare
			 * objects (no guest ar yet) carries an 'objects' list — the
			 * registry's zlib does — and cc expands it */
			char probe[512], line[512];
			int pfd;
			snprint(probe, sizeof probe, "/lib/wasm32-wasi/lib%s.a", s+2);
			if((pfd = open(probe, OREAD)) >= 0){
				close(pfd);
				lflags[nlf++] = s;
			} else {
				snprint(probe, sizeof probe, "/lib/wasm32-wasi/lib%s.objects", s+2);
				if((pfd = open(probe, OREAD)) >= 0){
					int li = 0, rn;
					char ch;
					while((rn = read(pfd, &ch, 1)) > 0){
						if(ch == '\n'){
							line[li] = 0;
							if(li > 0 && nobj < MAXARG)
								objs[nobj++] = strdup(line);
							li = 0;
						} else if(li < (int)sizeof line - 1)
							line[li++] = ch;
					}
					close(pfd);
				} else
					lflags[nlf++] = s;	/* let the linker say so */
			}
		}
		else if(strncmp(s, "-L", 2) == 0)
			lflags[nlf++] = s;			/* linker */
		else if(len > 2 && strcmp(s+len-2, ".c") == 0)
			srcs[nsrc++] = s;			/* a source */
		else if(len > 2 && strcmp(s+len-2, ".o") == 0)
			objs[nobj++] = absolutize(s);		/* an object to link */
		else if(s[0] == '-')
			cflags[ncf++] = s;			/* forward to the compiler */
		else
			srcs[nsrc++] = s;			/* bare word: treat as source */
	}

	if(nsrc == 0 && nobj == 0){
		fprint(2, "usage: cc [-c] [-o out] [flags] file.c ...\n");
		exits("usage");
	}
	if((fd = open("/bin/clang", OREAD)) < 0){
		fprint(2, "cc: the C toolchain has not finished arriving — it streams in\n");
		fprint(2, "    after boot (watch the top-right corner); try again shortly.\n");
		exits("no toolchain");
	}
	close(fd);

	/* compile each source */
	for(i = 0; i < nsrc; i++){
		char *obj = compileonly && out ? out : tosuf(srcs[i], "/tmp/", ".o");
		na = 0;
		for(j = 0; j < nelem(ccpre); j++){
			if(stopat && strcmp(ccpre[j], "-emit-obj") == 0)
				continue;			/* -E/-S replace the object step */
			a[na++] = ccpre[j];
		}
		if(stopat) a[na++] = stopat;
		for(j = 0; j < ncf; j++) a[na++] = cflags[j];
		if(strcmp(stopat ? stopat : "", "-E") != 0){	/* -E prints to stdout */
			a[na++] = "-o";
			a[na++] = stopat ? (out ? absolutize(out) : absolutize(tosuf(srcs[i], "", ".s"))) : obj;
		}
		a[na++] = "-x"; a[na++] = "c"; a[na++] = absolutize(srcs[i]);
		a[na] = nil;
		if(!run(a)){ fprint(2, "cc: %s: compilation failed\n", srcs[i]); exits("compile"); }
		objs[nobj++] = obj;
	}
	if(compileonly || stopat) exits(nil);

	/* link */
	na = 0;
	for(j = 0; j < nelem(ldpre); j++) a[na++] = ldpre[j];
	for(j = 0; j < nobj; j++) a[na++] = objs[j];
	a[na++] = "-lc";
	for(j = 0; j < nlf; j++) a[na++] = lflags[j];
	a[na++] = "-o"; a[na++] = out ? absolutize(out) : absolutize("a.out");
	a[na] = nil;
	if(!run(a)){ fprint(2, "cc: link failed\n"); exits("link"); }
	exits(nil);
	return 0;
}
