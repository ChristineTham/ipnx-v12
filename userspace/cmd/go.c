/* go: a real Go toolchain driver for ipnx.
 *
 * The gc compiler and linker are pure Go programs, so they cross-build to
 * wasip1 and run here as ordinary guests: /go/bin/compile and /go/bin/link,
 * with the standard library's export archives under /go/pkg (the package set
 * is derived from gobyexample.com's imports — the measured benchmark).
 * `go build` upstream cannot run on WASI because it orchestrates through
 * os/exec; this driver supplies exactly that orchestration with ipnx's own
 * fork+exec, the same way cc(1) drives clang. The tools are the real tools.
 *
 *   go build [-o out] f.go...   compile and link (out defaults to f)
 *   go run f.go [args...]       build to /tmp and exec
 *   go fmt f.go...              the real gofmt, -l -w
 *   go version | env [VAR]
 */
#include "lib9.h"

#define nelem(x) (sizeof(x)/sizeof((x)[0]))
enum { MAXARG = 256 };

static char *
absolutize(char *s)
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

static void child(void *v){ char **b = v; exec(b[0], b); exits("exec"); }

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

static void
needtoolchain(void)
{
	int fd;

	if((fd = open("/go/bin/compile", OREAD)) >= 0){ close(fd); return; }
	fprint(2, "go: the Go toolchain is not loaded — boot the demo with the\n");
	fprint(2, "    \"Go toolchain\" button (it adds the real compiler and linker,\n");
	fprint(2, "    cross-built to wasm, plus the standard library).\n");
	exits("no toolchain");
}

static char *
version(void)
{
	static char v[64];
	int fd, n;

	fd = open("/go/VERSION", OREAD);
	if(fd < 0) return "go1.25.6";
	n = read(fd, v, sizeof v - 1);
	close(fd);
	if(n <= 0) return "go1.25.6";
	while(n > 0 && (v[n-1] == '\n' || v[n-1] == '\r')) n--;
	v[n] = 0;
	return v;
}

/* compile srcs into obj, link into out; both absolute */
static void
gobuild(char **srcs, int nsrc, char *out)
{
	char *a[MAXARG]; int na, i;

	needtoolchain();
	na = 0;
	a[na++] = "/go/bin/compile"; a[na++] = "-p"; a[na++] = "main";
	a[na++] = "-importcfg"; a[na++] = "/go/importcfg";
	a[na++] = "-o"; a[na++] = "/tmp/_go.o";
	for(i = 0; i < nsrc; i++) a[na++] = absolutize(srcs[i]);
	a[na] = nil;
	if(!run(a)) exits("compile");
	na = 0;
	a[na++] = "/go/bin/link"; a[na++] = "-importcfg"; a[na++] = "/go/importcfg";
	a[na++] = "-o"; a[na++] = out;
	a[na++] = "/tmp/_go.o";
	a[na] = nil;
	if(!run(a)) exits("link");
}

int
main(int argc, char *argv[])
{
	char *cmd = argc > 1 ? argv[1] : "";
	char *srcs[MAXARG]; int nsrc = 0;
	char *out = nil;
	int i;

	if(strcmp(cmd, "version") == 0){
		print("go version %s wasip1/wasm\n", version());
		exits(nil);
	}
	if(strcmp(cmd, "env") == 0){
		char *v = argc > 2 ? argv[2] : nil;
		if(v == nil)
			print("GOOS='wasip1'\nGOARCH='wasm'\nGOVERSION='%s'\nGOROOT='/go'\n", version());
		else if(strcmp(v,"GOOS")==0) print("wasip1\n");
		else if(strcmp(v,"GOARCH")==0) print("wasm\n");
		else if(strcmp(v,"GOVERSION")==0) print("%s\n", version());
		else if(strcmp(v,"GOROOT")==0) print("/go\n");
		else print("\n");
		exits(nil);
	}
	if(strcmp(cmd, "fmt") == 0){
		char *a[MAXARG]; int na = 0;
		needtoolchain();
		a[na++] = "/go/bin/gofmt"; a[na++] = "-l"; a[na++] = "-w";
		for(i = 2; i < argc && na < MAXARG-1; i++) a[na++] = absolutize(argv[i]);
		a[na] = nil;
		exec(a[0], a);
		exits("exec");
	}
	if(strcmp(cmd, "build") == 0){
		for(i = 2; i < argc; i++){
			if(strcmp(argv[i], "-o") == 0 && i+1 < argc) out = argv[++i];
			else srcs[nsrc++] = argv[i];
		}
		if(nsrc == 0){ fprint(2, "go build: no Go files listed\n"); exits("usage"); }
		if(out == nil){
			char *b = strrchr(srcs[0], '/'), *dot;
			b = b ? b+1 : srcs[0];
			out = strdup(b);
			dot = strrchr(out, '.');
			if(dot && strcmp(dot, ".go") == 0) *dot = 0;
		}
		gobuild(srcs, nsrc, absolutize(out));
		exits(nil);
	}
	if(strcmp(cmd, "run") == 0){
		char *a[MAXARG]; int na = 0, j = 2;
		for(i = 2; i < argc; i++){
			int l = strlen(argv[i]);
			if(l > 3 && strcmp(argv[i]+l-3, ".go") == 0){ srcs[nsrc++] = argv[i]; j = i+1; }
			else break;
		}
		if(nsrc == 0){ fprint(2, "go run: no Go files listed\n"); exits("usage"); }
		gobuild(srcs, nsrc, "/tmp/_gorun");
		a[na++] = "/tmp/_gorun";
		for(i = j; i < argc && na < MAXARG-1; i++) a[na++] = argv[i];
		a[na] = nil;
		exec(a[0], a);
		exits("exec");
	}
	fprint(2, "usage: go build [-o out] f.go... | run f.go [args] | fmt f.go... | version | env [VAR]\n");
	fprint(2, "note: the compiler and linker are the real gc tools, cross-built to\n");
	fprint(2, "      wasm and run as ipnx processes; `go help ipnx` is cc(1)'s story.\n");
	exits(nil);
	return 0;
}
