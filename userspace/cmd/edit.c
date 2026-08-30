/* edit: acme-today — the one editor, as a canvas workspace (M5,
 * docs/userland.md). The shape is acme's: a root tag, a column, windows
 * of tag-and-body; the tag's file name is EDITABLE (Put writes to
 * whatever it says — acme's own truth), Get re-reads, Del closes the
 * window, New opens another; the Edit box runs sam's language on the
 * real libregexp. The colours are acme's: #eaffff tags, #ffffea bodies.
 * Layout, wrapping, IME: the presenter's. Word-granular tag execution
 * arrives with span roles; today the verbs are honest buttons.
 *
 * Node ids, stable for scripts and the suite: root tag 1, New 2,
 * column 3; window k at base 10+10k: wrapper, tagrow, fname(+2),
 * Get(+3), Put(+4), Del(+5), cmdbox(+6), EditBtn(+7), body(+8).
 */
#include <u.h>
#include <libc.h>
#include <regexp.h>

enum {
	MAXWIN = 8,
	BMAX = 32768,
	NMAX = 256,
	CMAX = 512,
	BASE = 10,
	STRIDE = 10,
};

typedef struct Wn Wn;
struct Wn {
	int used;
	char name[NMAX];
	long nlen;
	char cmd[CMAX];
	long clen;
	char body[BMAX];
	long blen;
};

static Wn wns[MAXWIN];
static char wdir[64];
static int ctlfd;

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

static void
nwrite(int id, char *fname, char *data, long n)
{
	char path[96];
	int fd;

	snprint(path, sizeof path, "%s/canvas/%d/%s", wdir, id, fname);
	fd = open(path, OWRITE);
	if(fd >= 0){
		write(fd, data, n);
		close(fd);
	}
}

static void
nprint(int id, char *fname, char *s)
{
	nwrite(id, fname, s, strlen(s));
}

static void
setnode(int id, char *text, long n)
{
	nprint(id, "addr", "0,$");
	nwrite(id, "data", text, n);
}

static long
readfile(char *path, char *buf, long max)
{
	int fd;
	long n, got;

	fd = open(path, OREAD);
	if(fd < 0)
		return -1;
	got = 0;
	while(got < max - 1 && (n = read(fd, buf + got, max - 1 - got)) > 0)
		got += n;
	close(fd);
	buf[got] = 0;
	return got;
}

/* ---- sam's language, the v0 core (docs/userland.md) ---- */

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

static int
collect(Wn *w, Reprog *prog, Span *sp, int max, Resub *subs, int nsub)
{
	long off;
	int n;
	Resub m[10];

	n = 0;
	off = 0;
	w->body[w->blen] = 0;
	while(off <= w->blen && n < max){
		memset(m, 0, sizeof m);
		if(!regexec(prog, w->body + off, m, nsub))
			break;
		sp[n].q0 = m[0].sp - w->body;
		sp[n].q1 = m[0].ep - w->body;
		if(subs != nil)
			memmove(&subs[n * 10], m, sizeof m);
		n++;
		off = (m[0].ep - w->body) + (m[0].sp == m[0].ep ? 1 : 0);
	}
	return n;
}

static void
splice(Wn *w, long q0, long q1, char *txt, long tn)
{
	if(w->blen - (q1 - q0) + tn > BMAX - 1)
		return;
	memmove(w->body + q0 + tn, w->body + q1, w->blen - q1);
	memmove(w->body + q0, txt, tn);
	w->blen += tn - (q1 - q0);
	w->body[w->blen] = 0;
}

static void
runedit(Wn *w, char *line)
{
	static Span sp[512];
	static Resub subs[512 * 10];
	char re[256], arg[512], out[1024];
	Reprog *prog;
	char *p;
	int n, i, glob;

	p = line;
	while(*p == ' ' || *p == '\t' || *p == ',')
		p++;
	if(*p == 'x'){
		p = delim(p + 1, re, sizeof re);
		if(p == nil)
			return;
		prog = regcomp(re);
		if(prog == nil)
			return;
		if(*p == 'c'){
			if(delim(p + 1, arg, sizeof arg) == nil){ free(prog); return; }
			n = collect(w, prog, sp, 512, nil, 1);
			for(i = n - 1; i >= 0; i--)
				splice(w, sp[i].q0, sp[i].q1, arg, strlen(arg));
		} else if(*p == 'd'){
			n = collect(w, prog, sp, 512, nil, 1);
			for(i = n - 1; i >= 0; i--)
				splice(w, sp[i].q0, sp[i].q1, "", 0);
		}
		free(prog);
		return;
	}
	if(*p == 's'){
		p = delim(p + 1, re, sizeof re);
		if(p == nil)
			return;
		p = delim(p - 1, arg, sizeof arg);	/* p-1 is the middle slash */
		if(p == nil)
			return;
		glob = *p == 'g';
		prog = regcomp(re);
		if(prog == nil)
			return;
		n = collect(w, prog, sp, 512, subs, 10);
		if(n > 0 && !glob)
			n = 1;
		for(i = n - 1; i >= 0; i--){
			regsub(arg, out, sizeof out, &subs[i * 10], 10);
			splice(w, sp[i].q0, sp[i].q1, out, strlen(out));
		}
		free(prog);
	}
}

/* ---- the workspace ---- */

static void
mkwin(int k, char *file)
{
	Wn *w;
	int base;
	char b[4096];
	long n;

	w = &wns[k];
	base = BASE + STRIDE * k;
	memset(w, 0, sizeof *w);
	w->used = 1;
	if(file != nil){
		w->nlen = snprint(w->name, sizeof w->name, "%s", file);
		n = readfile(file, w->body, sizeof w->body);
		if(n >= 0)
			w->blen = n;
	}
	n = snprint(b, sizeof b,
		"new %d stack\nnew %d stack\nnew %d edit\nnew %d text\nnew %d text\nnew %d text\nnew %d edit\nnew %d text\nnew %d edit\n",
		base, base + 1, base + 2, base + 3, base + 4, base + 5, base + 6, base + 7, base + 8);
	write(ctlfd, b, n);
	/* wrapper under the column; tag row under the wrapper; acme's colours */
	nprint(base, "attrs", "parent=3\n");
	{
		char a[256];
		snprint(a, sizeof a, "parent=%d\ndir=row\nbg=#eaffff\norder=1\n", base);
		nprint(base + 1, "attrs", a);
		snprint(a, sizeof a, "parent=%d\nbg=#eaffff\norder=1\n", base + 1);
		nprint(base + 2, "attrs", a);
		snprint(a, sizeof a, "parent=%d\naction=execute\norder=2\n", base + 1);
		nprint(base + 3, "attrs", a);
		snprint(a, sizeof a, "parent=%d\naction=execute\norder=3\n", base + 1);
		nprint(base + 4, "attrs", a);
		snprint(a, sizeof a, "parent=%d\naction=execute\norder=4\n", base + 1);
		nprint(base + 5, "attrs", a);
		snprint(a, sizeof a, "parent=%d\nbg=#eaffff\norder=5\n", base + 1);
		nprint(base + 6, "attrs", a);
		snprint(a, sizeof a, "parent=%d\naction=execute\norder=6\n", base + 1);
		nprint(base + 7, "attrs", a);
		snprint(a, sizeof a, "parent=%d\nbg=#ffffea\norder=2\n", base);
		nprint(base + 8, "attrs", a);
	}
	setnode(base + 2, w->name, w->nlen);
	nprint(base + 3, "data", "Get");
	nprint(base + 4, "data", "Put");
	nprint(base + 5, "data", "Del");
	nprint(base + 7, "data", "Edit");
	setnode(base + 8, w->body, w->blen);
}

static int
freewin(void)
{
	int k;

	for(k = 0; k < MAXWIN; k++)
		if(!wns[k].used)
			return k;
	return -1;
}

void
main(int argc, char *argv[])
{
	char path[128], line[4096], txt[4096], *f[8];
	int wid, evfd, fd, nf, i, k, base, off, minted;
	long n, q0, q1, d;
	Wn *w;

	wid = -1;
	i = 1;
	if(argc > 2 && strcmp(argv[1], "-W") == 0){
		wid = atoi(argv[2]);
		i = 3;
	}

	minted = 0;
	if(wid < 0){
		fd = open("#w/clone", OREAD);
		if(fd < 0)
			sysfatal("no window server: %r");
		n = read(fd, path, 15);
		close(fd);
		if(n <= 0)
			sysfatal("clone read failed");
		path[n] = 0;
		wid = atoi(path);
		minted = 1;
	}
	USED(minted);
	snprint(wdir, sizeof wdir, "#w/%d", wid);

	snprint(path, sizeof path, "%s/canvas/ctl", wdir);
	ctlfd = open(path, OWRITE);
	if(ctlfd < 0)
		sysfatal("no canvas on this host: %r");

	/* the root tag and the column */
	fprint(ctlfd, "new 1 stack\nnew 2 text\nnew 3 stack\n");
	nprint(1, "attrs", "dir=row\nbg=#eaffff\norder=1\n");
	nprint(2, "attrs", "parent=1\naction=execute\n");
	nprint(2, "data", "New");
	nprint(3, "attrs", "order=2\n");

	k = 0;
	for(; i < argc && k < MAXWIN; i++, k++)
		mkwin(k, argv[i]);
	if(k == 0)
		mkwin(k++, nil);
	fprint(ctlfd, "sync\n");

	snprint(path, sizeof path, "%s/label", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "edit — acme-today"); close(fd); }

	snprint(path, sizeof path, "%s/canvas/events", wdir);
	evfd = open(path, OREAD);
	if(evfd < 0)
		sysfatal("canvas events: %r");

	for(;;){
		n = read(evfd, line, sizeof line - 1);	/* one event per read */
		if(n <= 0)
			break;
		line[n] = 0;
		nf = tokenize(line, f, 8);
		if(nf < 2)
			continue;
		if(strcmp(f[0], "close") == 0)
			break;
		i = atoi(f[1]);
		if(strcmp(f[0], "execute") == 0 && i == 2){	/* New */
			k = freewin();
			if(k >= 0){
				mkwin(k, nil);
				fprint(ctlfd, "sync\n");
			}
			continue;
		}
		if(i < BASE)
			continue;
		k = (i - BASE) / STRIDE;
		off = (i - BASE) % STRIDE;
		if(k >= MAXWIN || !wns[k].used)
			continue;
		w = &wns[k];
		base = BASE + STRIDE * k;
		if(nf >= 4 && strcmp(f[0], "insert") == 0){
			char *buf = off == 2 ? w->name : off == 6 ? w->cmd : off == 8 ? w->body : nil;
			long *len = off == 2 ? &w->nlen : off == 6 ? &w->clen : off == 8 ? &w->blen : nil;
			long max = off == 2 ? NMAX : off == 6 ? CMAX : BMAX;
			if(buf == nil)
				continue;
			q0 = atol(f[2]);
			d = unq(f[3], txt, sizeof txt);
			if(q0 > *len) q0 = *len;
			if(*len + d > max - 1) d = max - 1 - *len;
			memmove(buf + q0 + d, buf + q0, *len - q0);
			memmove(buf + q0, txt, d);
			*len += d;
			buf[*len] = 0;
			continue;
		}
		if(nf >= 4 && strcmp(f[0], "delete") == 0){
			char *buf = off == 2 ? w->name : off == 6 ? w->cmd : off == 8 ? w->body : nil;
			long *len = off == 2 ? &w->nlen : off == 6 ? &w->clen : off == 8 ? &w->blen : nil;
			if(buf == nil)
				continue;
			q0 = atol(f[2]);
			q1 = atol(f[3]);
			if(q1 > *len) q1 = *len;
			if(q0 > q1) q0 = q1;
			memmove(buf + q0, buf + q1, *len - q1);
			*len -= q1 - q0;
			buf[*len] = 0;
			continue;
		}
		if(strcmp(f[0], "execute") != 0)
			continue;
		switch(off){
		case 3:				/* Get: re-read the tag's file */
			n = readfile(w->name, w->body, sizeof w->body);
			if(n >= 0){
				w->blen = n;
				setnode(base + 8, w->body, w->blen);
				fprint(ctlfd, "sync\n");
			}
			break;
		case 4:				/* Put: write to whatever the tag says */
			if(w->nlen == 0)
				break;
			fd = create(w->name, OWRITE, 0644);
			if(fd >= 0){
				write(fd, w->body, w->blen);
				close(fd);
			}
			break;
		case 5:				/* Del: this window leaves the column */
			fprint(ctlfd, "del %d\nsync\n", base);
			w->used = 0;
			break;
		case 7:				/* Edit: sam's language on the body */
			runedit(w, w->cmd);
			setnode(base + 8, w->body, w->blen);
			fprint(ctlfd, "sync\n");
			break;
		}
	}
	snprint(path, sizeof path, "%s/wctl", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "delete"); close(fd); }
	exits(0);
}
