/* con: the console as an editable transcript — console-today (M5,
 * docs/userland.md). The transcript is one canvas edit node; a mark
 * separates history from the input region; Enter sends the line to the
 * command; everything above the mark is ordinary text — scrollback is
 * the buffer, search is the editor, copy is selection. win(1) was this
 * design's prototype. AND, not XOR (2026-08-30): the xterm byte console
 * stays beside this one — con is the native design, not the only door.
 *
 * Shape: three coroutines on the wasm libthread. A pipe reader and an
 * event reader feed one consumer through a channel, so a single thread
 * owns every canvas write and all mark arithmetic — the transcript
 * needs no locks and is never re-read (shadow state only). A tiny
 * presenter/device offset race between an output insert and an unsynced
 * local echo is accepted for v0 and noted here.
 */
#include <u.h>
#include <libc.h>
#include <thread.h>

enum { STACK = 8192, INMAX = 4096 };

typedef struct Msg Msg;
struct Msg {
	int kind;			/* 0 output, 1 event line, 2 stream gone */
	int n;
	char *buf;
};

static Channel *mc;			/* Msg* */
static Channel *pidc;			/* ulong: the child's pid */
static int outfd[2], infd[2];
static int ctlfd, addrfd, datafd, evfd;
static char *cpath;
static char **cargv;

static void
execproc(void *v)
{
	USED(v);
	rfork(RFFDG);		/* intercepted by the port: stashed, restored post-fork */
	dup(infd[0], 0);
	dup(outfd[1], 1);
	dup(outfd[1], 2);
	close(infd[0]); close(infd[1]);
	close(outfd[0]); close(outfd[1]);
	procexec(pidc, cpath, cargv);
	threadexits("exec");
}

static void
outproc(void *v)
{
	Msg *m;
	char buf[1024];
	long n;

	USED(v);
	for(;;){
		n = read(outfd[0], buf, sizeof buf);
		m = malloc(sizeof(Msg));
		m->kind = n > 0 ? 0 : 2;
		m->n = n > 0 ? n : 0;
		m->buf = malloc(m->n + 1);
		if(m->n > 0)
			memmove(m->buf, buf, m->n);
		m->buf[m->n] = 0;
		sendp(mc, m);
		if(n <= 0)
			threadexits(nil);
	}
}

static void
evproc(void *v)
{
	Msg *m;
	char buf[4096];
	long n;

	USED(v);
	for(;;){
		n = read(evfd, buf, sizeof buf);	/* one event line per read */
		m = malloc(sizeof(Msg));
		m->kind = n > 0 ? 1 : 2;
		m->n = n > 0 ? n : 0;
		m->buf = malloc(m->n + 1);
		if(m->n > 0)
			memmove(m->buf, buf, m->n);
		m->buf[m->n] = 0;
		sendp(mc, m);
		if(n <= 0)
			threadexits(nil);
	}
}

/* %-quoting undone: %20 space, %0A newline, %25 percent */
static int
unq(char *s, char *out, int max)
{
	int n;

	n = 0;
	while(*s != 0 && *s != '\n' && n < max - 1){
		if(s[0] == '%' && s[1] != 0 && s[2] != 0){
			if(s[1] == '2' && s[2] == '0'){ out[n++] = ' '; s += 3; continue; }
			if(s[1] == '0' && (s[2] == 'A' || s[2] == 'a')){ out[n++] = '\n'; s += 3; continue; }
			if(s[1] == '2' && s[2] == '5'){ out[n++] = '%'; s += 3; continue; }
		}
		out[n++] = *s++;
	}
	out[n] = 0;
	return n;
}

void
threadmain(int argc, char *argv[])
{
	static char *rcav[] = { "rc", "-i", nil };
	static char xpath[128];		/* the exec path — never a scratch buffer */
	char path[128], wdir[64], in[INMAX], txt[4096], *f[8], *base;
	int wid, nf, i, inlen, minted;
	long mark, blen, q0, q1, d, pos, ln;
	ulong pid;
	Msg *m;
	char *nl;

	wid = -1;
	i = 1;
	if(argc > 2 && strcmp(argv[1], "-W") == 0){
		wid = atoi(argv[2]);
		i = 3;
	}
	if(i < argc){
		cpath = argv[i];
		cargv = &argv[i];
	} else {
		cpath = "/bin/rc";
		cargv = rcav;
	}
	snprint(xpath, sizeof xpath, "%s%s", cpath[0] == '/' ? "" : "/bin/", cpath);
	cpath = xpath;

	minted = 0;
	if(wid < 0){
		int cfd = open("#w/clone", OREAD);
		char cb[16];
		long cn;
		if(cfd < 0)
			sysfatal("no window server: %r");
		cn = read(cfd, cb, sizeof cb - 1);
		close(cfd);
		if(cn <= 0)
			sysfatal("clone read failed");
		cb[cn] = 0;
		wid = atoi(cb);
		minted = 1;
	}
	snprint(wdir, sizeof wdir, "#w/%d", wid);

	snprint(path, sizeof path, "%s/canvas/ctl", wdir);
	ctlfd = open(path, OWRITE);
	if(ctlfd < 0)
		sysfatal("no canvas on this host: %r");
	fprint(ctlfd, "new 1 edit\nsync\n");
	snprint(path, sizeof path, "%s/canvas/1/addr", wdir);
	addrfd = open(path, OWRITE);
	snprint(path, sizeof path, "%s/canvas/1/data", wdir);
	datafd = open(path, OWRITE);
	snprint(path, sizeof path, "%s/canvas/events", wdir);
	evfd = open(path, OREAD);
	if(addrfd < 0 || datafd < 0 || evfd < 0)
		sysfatal("canvas files: %r");

	base = strrchr(cargv[0], '/');
	snprint(path, sizeof path, "%s/label", wdir);
	i = open(path, OWRITE);
	if(i >= 0){
		fprint(i, "con %s", base ? base + 1 : cargv[0]);
		close(i);
	}

	if(pipe(outfd) < 0 || pipe(infd) < 0)
		sysfatal("pipe: %r");
	mc = chancreate(sizeof(Msg*), 8);
	pidc = chancreate(sizeof(ulong), 1);
	proccreate(execproc, nil, STACK);
	pid = recvul(pidc);
	close(infd[0]);
	close(outfd[1]);
	threadcreate(outproc, nil, STACK);
	threadcreate(evproc, nil, STACK);

	mark = 0;
	blen = 0;
	inlen = 0;
	for(;;){
		m = recvp(mc);
		switch(m->kind){
		case 0:				/* command output: insert at the mark */
			fprint(addrfd, "%ld,%ld", mark, mark);
			write(datafd, m->buf, m->n);
			mark += m->n;
			blen += m->n;
			fprint(ctlfd, "sync");
			break;
		case 1: {			/* the user edited the transcript */
			nf = tokenize(m->buf, f, nelem(f));
			if(nf >= 3 && strcmp(f[0], "insert") == 0){
				q0 = atol(f[2]);
				ln = nf >= 4 ? unq(f[3], txt, sizeof txt) : 0;
				blen += ln;
				if(q0 < mark)
					mark += ln;	/* editing history shifts the mark */
				else {
					pos = q0 - mark;
					if(pos > inlen) pos = inlen;
					if(inlen + ln > INMAX - 1) ln = INMAX - 1 - inlen;
					memmove(in + pos + ln, in + pos, inlen - pos);
					memmove(in + pos, txt, ln);
					inlen += ln;
					while((nl = memchr(in, '\n', inlen)) != nil){
						d = nl - in + 1;
						write(infd[1], in, d);
						mark += d;	/* the sent line becomes history */
						memmove(in, in + d, inlen - d);
						inlen -= d;
					}
				}
				break;
			}
			if(nf >= 4 && strcmp(f[0], "delete") == 0){
				q0 = atol(f[2]);
				q1 = atol(f[3]);
				d = q1 - q0;
				blen -= d;
				if(q1 <= mark)
					mark -= d;
				else if(q0 >= mark){
					pos = q0 - mark;
					if(pos + d > inlen) d = inlen - pos;
					memmove(in + pos, in + pos + d, inlen - pos - d);
					inlen -= d;
				} else {		/* straddle: both sides shrink */
					d = q1 - mark;
					if(d > inlen) d = inlen;
					memmove(in, in + d, inlen - d);
					inlen -= d;
					mark = q0;
				}
				break;
			}
			if(strcmp(f[0], "close") == 0){
				postnote(PNPROC, pid, "kill");
				if(minted){
					snprint(path, sizeof path, "%s/wctl", wdir);
					i = open(path, OWRITE);
					if(i >= 0){ fprint(i, "delete"); close(i); }
				}
				threadexitsall(nil);
			}
			break;			/* resize, execute, look: v0 ignores */
		}
		case 2:				/* the command exited */
			fprint(addrfd, "%ld,%ld", mark, mark);
			fprint(datafd, "\n[exited]\n");
			fprint(ctlfd, "sync");
			threadexitsall(nil);
		}
		free(m->buf);
		free(m);
	}
}
