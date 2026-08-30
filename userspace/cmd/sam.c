/* sam: sam-today — the language as a filter, ed's true heir (M5,
 * docs/userland.md). The batch CLI that was sam -d's job: commands on
 * stdin, structural regexps on the real libregexp, the buffer piped
 * through real commands. It inherits the name by passing the
 * ancestor's own tests; the raster original steps back to sam9.
 *
 *   e /path            load
 *   ,x/re/ c/text/     change every match
 *   ,x/re/ g/re2/ ...  guard: keep matches also matching re2 (v/ negates)
 *   ,x/re/ d           delete every match
 *   ,s/re/sub/[g]      substitute (& and \1..\9)
 *   ,| cmd args        pipe the buffer through a command
 *   w [/path]          write
 *   q                  quit
 * -d is accepted and ignored: batch is the mode.
 */
#include <u.h>
#include <libc.h>
#include <thread.h>
#include <regexp.h>

enum { BMAX = 262144, NSPAN = 4096 };

static char body[BMAX];
static long blen;
static char fname[256];

typedef struct Span Span;
struct Span {
	long q0, q1;
};

static char *
delim(char *p, char *out, int max)
{
	int n;

	if(*p != '/')
		return nil;
	p++;
	n = 0;
	while(*p != 0 && *p != '/' && n < max - 1){
		if(*p == '\\' && p[1] == '/'){ out[n++] = '/'; p += 2; continue; }
		if(*p == '\\' && p[1] == 'n'){ out[n++] = '\n'; p += 2; continue; }
		out[n++] = *p++;
	}
	out[n] = 0;
	if(*p != '/')
		return nil;
	return p + 1;
}

static void
splice(long q0, long q1, char *txt, long tn)
{
	if(blen - (q1 - q0) + tn > BMAX - 1)
		return;
	memmove(body + q0 + tn, body + q1, blen - q1);
	memmove(body + q0, txt, tn);
	blen += tn - (q1 - q0);
	body[blen] = 0;
}

static long
loadfile(char *path)
{
	int fd;
	long n, got;

	fd = open(path, OREAD);
	if(fd < 0)
		return -1;
	got = 0;
	while(got < BMAX - 1 && (n = read(fd, body + got, BMAX - 1 - got)) > 0)
		got += n;
	close(fd);
	body[got] = 0;
	return got;
}

/* ,| cmd — the whole buffer through a command, con's proven shape:
 * the runproc's rfork(RFFDG) is intercepted and restored around the
 * real fork (the port's procexec contract), and a channel sequences
 * the spawn. Write-then-read is sequential; a command whose output
 * outruns the pipe against a still-filling input could stall at sizes
 * far beyond the language's use — noted, accepted. */
static int pin[2], pout[2];
static char *pipeav[16];
static char pipepath[128];
static Channel *ppidc;

static void
pxchild(void *v)
{
	USED(v);
	rfork(RFFDG);
	dup(pin[0], 0);
	dup(pout[1], 1);
	close(pin[0]); close(pin[1]);
	close(pout[0]); close(pout[1]);
	procexec(ppidc, pipepath, pipeav);
	threadexits("exec");
}

static void
pipebody(char *cmdline)
{
	int nav;
	long n, got;

	nav = tokenize(cmdline, pipeav, 15);
	if(nav < 1)
		return;
	pipeav[nav] = nil;
	snprint(pipepath, sizeof pipepath, "%s%s",
		pipeav[0][0] == '/' ? "" : "/bin/", pipeav[0]);
	if(pipe(pin) < 0 || pipe(pout) < 0)
		return;
	if(ppidc == nil)
		ppidc = chancreate(sizeof(ulong), 1);
	proccreate(pxchild, nil, 8192);
	recvul(ppidc);
	close(pin[0]);
	close(pout[1]);
	write(pin[1], body, blen);
	close(pin[1]);
	got = 0;
	while(got < BMAX - 1 && (n = read(pout[0], body + got, BMAX - 1 - got)) > 0)
		got += n;
	close(pout[0]);
	blen = got;
	body[blen] = 0;
}

static int
collect(Reprog *prog, Reprog *guard, int vneg, Span *sp, int max, Resub *subs)
{
	long off;
	int n, keep;
	Resub m[10];
	char save;

	n = 0;
	off = 0;
	body[blen] = 0;
	while(off <= blen && n < max){
		memset(m, 0, sizeof m);
		if(!regexec(prog, body + off, m, subs != nil ? 10 : 1))
			break;
		keep = 1;
		if(guard != nil){
			Resub g[1];
			memset(g, 0, sizeof g);
			save = *m[0].ep;
			*(char*)m[0].ep = 0;
			keep = regexec(guard, m[0].sp, g, 1);
			*(char*)m[0].ep = save;
			if(vneg)
				keep = !keep;
		}
		if(keep){
			sp[n].q0 = m[0].sp - body;
			sp[n].q1 = m[0].ep - body;
			if(subs != nil)
				memmove(&subs[n * 10], m, sizeof m);
			n++;
		}
		off = (m[0].ep - body) + (m[0].sp == m[0].ep ? 1 : 0);
	}
	return n;
}

static void
runline(char *line)
{
	static Span sp[NSPAN];
	static Resub subs[64 * 10];
	char re[256], re2[256], arg[1024], out[2048];
	Reprog *prog, *guard;
	char *p;
	int n, i, glob, vneg;

	p = line;
	while(*p == ' ' || *p == '\t')
		p++;
	if(*p == 0)
		return;
	if(strncmp(p, "e ", 2) == 0){
		p += 2;
		while(*p == ' ')
			p++;
		snprint(fname, sizeof fname, "%s", p);
		blen = loadfile(fname);
		if(blen < 0){
			blen = 0;
			fprint(2, "?%s\n", fname);
		}
		return;
	}
	if(*p == 'w' && (p[1] == 0 || p[1] == ' ')){
		char *t = p[1] == ' ' ? p + 2 : fname;
		int fd;
		while(*t == ' ')
			t++;
		if(*t == 0)
			t = fname;
		fd = create(t, OWRITE, 0644);
		if(fd >= 0){
			write(fd, body, blen);
			close(fd);
		} else
			fprint(2, "?%s\n", t);
		return;
	}
	if(*p == 'q' && p[1] == 0)
		exits(0);
	while(*p == ',')
		p++;
	while(*p == ' ')
		p++;
	if(*p == '|'){
		p++;
		while(*p == ' ')
			p++;
		pipebody(p);
		return;
	}
	if(*p == 'x'){
		p = delim(p + 1, re, sizeof re);
		if(p == nil)
			return;
		while(*p == ' ')
			p++;
		guard = nil;
		vneg = 0;
		if((*p == 'g' || *p == 'v') && p[1] == '/'){
			vneg = *p == 'v';
			p = delim(p + 1, re2, sizeof re2);
			if(p == nil)
				return;
			guard = regcomp(re2);
			while(*p == ' ')
				p++;
		}
		prog = regcomp(re);
		if(prog == nil)
			return;
		if(*p == 'c'){
			if(delim(p + 1, arg, sizeof arg) != nil){
				n = collect(prog, guard, vneg, sp, NSPAN, nil);
				for(i = n - 1; i >= 0; i--)
					splice(sp[i].q0, sp[i].q1, arg, strlen(arg));
			}
		} else if(*p == 'd'){
			n = collect(prog, guard, vneg, sp, NSPAN, nil);
			for(i = n - 1; i >= 0; i--)
				splice(sp[i].q0, sp[i].q1, "", 0);
		}
		free(prog);
		if(guard != nil)
			free(guard);
		return;
	}
	if(*p == 's'){
		p = delim(p + 1, re, sizeof re);
		if(p == nil)
			return;
		p = delim(p - 1, arg, sizeof arg);
		if(p == nil)
			return;
		glob = *p == 'g';
		prog = regcomp(re);
		if(prog == nil)
			return;
		n = collect(prog, nil, 0, sp, 64, subs);
		if(n > 0 && !glob)
			n = 1;
		for(i = n - 1; i >= 0; i--){
			regsub(arg, out, sizeof out, &subs[i * 10], 10);
			splice(sp[i].q0, sp[i].q1, out, strlen(out));
		}
		free(prog);
	}
}

void
threadmain(int argc, char *argv[])
{
	static char line[4096];
	int i, ll, ch;
	char c;

	for(i = 1; i < argc; i++){
		if(strcmp(argv[i], "-d") == 0)
			continue;			/* batch is the mode */
		snprint(fname, sizeof fname, "%s", argv[i]);
		blen = loadfile(fname);
		if(blen < 0)
			blen = 0;
	}
	ll = 0;
	for(;;){
		ch = read(0, &c, 1);
		if(ch <= 0)
			break;
		if(c == '\n'){
			line[ll] = 0;
			runline(line);
			ll = 0;
		} else if(ll < (int)sizeof line - 1)
			line[ll++] = c;
	}
	exits(0);
}
