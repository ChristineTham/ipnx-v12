/* acme: acme-today — the one editor, the name inherited by the
 * succession rule (design log: the behaviour suite is the proof; the
 * raster ancestor's substrate tests stay with acme9). A canvas
 * workspace (M5, docs/userland.md), rebuilt 2026-08-30 against the
 * acme paper's own examples (the fidelity pass): external commands
 * run in the window's directory with output to dir/+Errors, the
 * selection filters | < > work, Undo/Redo unwind by sequence number,
 * B3 takes sam addresses (file:27, :/re/), Put appears in the tag
 * only while the window is dirty (the paper's rule), and the editor
 * serves its file interface — index, new, N/{tag,body,addr,data,
 * ctl,event} — as 9P posted at /srv/acme (mount acme /mnt/acme).
 *
 * Shape: con(1)'s libthread discipline — reader threads (canvas
 * events, the 9P pipe, each command's output) feed one consumer
 * through a channel; the consumer owns every shadow buffer and every
 * canvas write. Shadow only: buffers are never re-read from the
 * device. svc(4)'s server loop is the 9P half's pattern.
 *
 * Node ids, stable for scripts and the suite: root tag 1, colrow 3;
 * column c at 200+10c {stack, tag +1}; window k at base 10+10k:
 * wrapper, tag +1, body +2, tagrow +3, dirtybox +4. The 9P window id
 * is the canvas base, so the two trees correlate.
 */
#include <u.h>
#include <libc.h>
#include <thread.h>
#include <regexp.h>
#include "../libc/lib9p.h"

enum {
	MAXWIN = 12,
	MAXCOL = 4,
	MAXCMD = 6,
	BMAX = 32768,
	NMAX = 256,
	CMAX = 512,
	UMAX = 65536,		/* per-window undo/redo arena cap */
	EVMAX = 4096,		/* queued event-file bytes per window */
	NFID = 64,
	STACK = 8192,
	BASE = 10,
	STRIDE = 10,
	CBASE = 200,
	CSTRIDE = 10,
	/* 9P nodes: 0 root, 1 index, 2 new/, window k dir 100+10k, files +1.. */
	NINDEX = 1,
	NNEW = 2,
	WNODE = 100,
	WSTRIDE = 10,
	/* per-window file offsets within a window's 9P dir */
	FTAG = 1, FBODY = 2, FADDR = 3, FDATA = 4, FCTL = 5, FEVENT = 6,
	/* Msg kinds */
	MEV = 0, MFS = 1, MOUT = 2, MEOF = 3,
	/* mkwin content modes */
	MKREAD = 0, MKEMPTY = 1,
	/* command kinds */
	CPLAIN = 0, CPIPE = 1, CIN = 2, COUT = 3,
};

typedef struct Wn Wn;
struct Wn {
	int used;
	int col;
	int dirty;
	char name[NMAX];	/* the tag, whole */
	long nlen;
	char body[BMAX];
	long blen;
	long autopos, autolen;	/* the dynamic word block (Undo/Redo/Put) */
	long sq0, sq1;		/* body selection, from select events */
	long wq0, wq1;		/* the 9P addr file's range */
	uchar *ustk, *rstk;	/* undo and redo arenas */
	long ulen, rlen;
	ulong stateseq, cleanseq;
	int evopen;		/* the event file is held open */
	char evq[EVMAX];
	long evqlen;
	int evpend;		/* a parked Tread on event */
	int evtag;
	uint evcount;
};

typedef struct Col Col;
struct Col {
	int used;
	char tag[NMAX];
	long tlen;
};

typedef struct Cmd Cmd;
struct Cmd {
	int used;
	int pid;
	int kind;
	int targetk;		/* window whose selection | < > affect */
	long tq0, tq1;		/* that selection, captured at launch */
	char name[64];
	char dir[NMAX];
	char xpath[NMAX];
	char astore[CMAX];
	char *av[16];
	int outfd, infd;
	int pinw, poutr;	/* the parent's ends: the child must close them */
	char *ibuf;		/* selection bytes for the writer thread */
	long ilen;
	char *obuf;		/* collected output for | and < */
	long olen;
};

typedef struct Msg Msg;
struct Msg {
	int kind;
	int slot;		/* MOUT/MEOF: which command */
	long n;
	char *buf;
};

static Wn wns[MAXWIN];
static Col cols[MAXCOL];
static Cmd cmds[MAXCMD];
static int active;
static char wdir[64];
static int ctlfd, evfd, sfd = -1;
static Channel *mc;		/* Msg* */
static Channel *pidc;		/* ulong */
static ulong seq;
static int lastinsk = -1;	/* typed-insert coalescing state */
static char aroot[NMAX];	/* acme's own directory (root/column execs) */
static int fids[NFID], fidnode[NFID], fidopen[NFID];
static uchar fsmsg[MSIZE9], fsout[MSIZE9];

static void doexecute(int k, int c, char *word, char *args, int fromev);
static void dolook(int k, char *text, int fromev);

/* ---- small utilities ---- */

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

/* replace a device range without resending the node: addr then data */
static void
noderepl(int id, long q0, long q1, char *text, long n)
{
	char a[48];

	snprint(a, sizeof a, "%ld,%ld", q0, q1);
	nprint(id, "addr", a);
	nwrite(id, "data", text, n);
}

/* app -> surface: set the selection (and scroll it into view) */
static void
setsel(int k, long q0, long q1)
{
	char a[64];

	wns[k].sq0 = q0;
	wns[k].sq1 = q1;
	snprint(a, sizeof a, "sel=%ld,%ld\n", q0, q1);
	nprint(BASE + STRIDE * k + 2, "attrs", a);
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

static char *
firstword(char *t, long n, long *ln)
{
	static char w[NMAX];
	long i, j;

	i = 0;
	while(i < n && (t[i] == ' ' || t[i] == '\t'))
		i++;
	j = 0;
	while(i < n && t[i] != ' ' && t[i] != '\t' && t[i] != '\n' && j < NMAX - 1)
		w[j++] = t[i++];
	w[j] = 0;
	*ln = j;
	return w;
}

static int
isdir(char *path)
{
	Dir *d;
	int r;

	d = dirstat(path);
	if(d == nil)
		return -1;
	r = (d->mode & DMDIR) != 0;
	free(d);
	return r;
}

/* the directory a window's commands and looks resolve in */
static void
windirof(int k, char *out, int max)
{
	char *fn, *sl;
	long fl;

	if(k < 0){
		snprint(out, max, "%s", aroot);
		return;
	}
	fn = firstword(wns[k].name, wns[k].nlen, &fl);
	snprint(out, max, "%s", fn);
	if(fl > 0 && out[fl - 1] == '/')
		out[fl - 1] = 0;
	else if((sl = strrchr(out, '/')) != nil)
		*sl = 0;
	else
		snprint(out, max, "%s", aroot);
	if(out[0] == 0)
		snprint(out, max, "/");
}

/* ---- the undo machinery (the paper: sequence numbers group a unit) ----
 * A record says "the text at q0 changed a -> b":
 * [q0][alen][blen][seq][a bytes][b bytes][reclen]; push and pop at the
 * arena's tail, drop from the front when full. Undo moves the record to
 * the redo arena verbatim; redo moves it back. */

static void
put4(uchar *p, ulong v)
{
	p[0] = v; p[1] = v >> 8; p[2] = v >> 16; p[3] = v >> 24;
}

static ulong
get4(uchar *p)
{
	return p[0] | p[1] << 8 | p[2] << 16 | (ulong)p[3] << 24;
}

static void
pushrec(uchar **stk, long *len, long q0, char *a, long alen, char *b, long blen, ulong sq)
{
	long rl;
	uchar *p;

	rl = 16 + alen + blen + 4;
	if(rl > UMAX)
		return;
	if(*stk == nil)
		*stk = malloc(UMAX);
	while(*len + rl > UMAX){		/* drop the oldest record */
		long first = 16 + get4(*stk + 4) + get4(*stk + 8) + 4;
		memmove(*stk, *stk + first, *len - first);
		*len -= first;
	}
	p = *stk + *len;
	put4(p, q0); put4(p + 4, alen); put4(p + 8, blen); put4(p + 12, sq);
	memmove(p + 16, a, alen);
	memmove(p + 16 + alen, b, blen);
	put4(p + 16 + alen + blen, rl);
	*len += rl;
}

static ulong
topseq(uchar *stk, long len)
{
	long rl;

	if(len == 0)
		return 0;
	rl = get4(stk + len - 4);
	return get4(stk + len - rl + 12);
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

/* a recorded change: capture old, splice, push, stamp */
static void
change(int k, long q0, long q1, char *txt, long tn, ulong sq)
{
	Wn *w = &wns[k];
	static char old[BMAX];
	long olen;

	if(q0 < 0) q0 = 0;
	if(q1 > w->blen) q1 = w->blen;
	if(q0 > q1) q0 = q1;
	olen = q1 - q0;
	memmove(old, w->body + q0, olen);
	splice(w, q0, q1, txt, tn);
	pushrec(&w->ustk, &w->ulen, q0, old, olen, txt, tn, sq);
	w->rlen = 0;			/* a fresh change clears redo */
	w->stateseq = sq;
	lastinsk = -1;
}

/* extend the top record's b-side by n bytes just appended at its end —
 * consecutive typing undoes as one unit */
static int
extendtop(Wn *w, long q0, char *txt, long n)
{
	long rl, rq0, ral, rbl;
	uchar *p;

	if(w->ulen == 0 || w->rlen != 0)
		return 0;
	rl = get4(w->ustk + w->ulen - 4);
	p = w->ustk + w->ulen - rl;
	rq0 = get4(p); ral = get4(p + 4); rbl = get4(p + 8);
	if(ral != 0 || rq0 + rbl != q0 || rl + n > UMAX || w->ulen + n > UMAX)
		return 0;
	memmove(p + 16 + rbl, txt, n);
	put4(p + 8, rbl + n);
	put4(p + 16 + rbl + n, rl + n);
	w->ulen += n;
	return 1;
}

/* ---- the dynamic tag words (the paper: Put appears when dirty) ---- */

static void
rebuildauto(int k)
{
	Wn *w = &wns[k];
	char want[64], *fn;
	long fl, wl;
	int dirw;

	fn = firstword(w->name, w->nlen, &fl);
	dirw = fl > 0 && fn[fl - 1] == '/';
	want[0] = 0;
	if(w->ulen > 0)
		strcat(want, " Undo");
	if(w->rlen > 0)
		strcat(want, " Redo");
	if(w->dirty && !dirw)
		strcat(want, " Put");
	wl = strlen(want);
	if(w->autopos > w->nlen)
		w->autopos = w->nlen;
	if(w->autopos + w->autolen > w->nlen)
		w->autolen = w->nlen - w->autopos;
	if(wl == w->autolen && memcmp(w->name + w->autopos, want, wl) == 0)
		return;
	noderepl(BASE + STRIDE * k + 1, w->autopos, w->autopos + w->autolen, want, wl);
	if(w->nlen - w->autolen + wl > NMAX - 1)
		return;
	memmove(w->name + w->autopos + wl, w->name + w->autopos + w->autolen,
		w->nlen - w->autopos - w->autolen);
	memmove(w->name + w->autopos, want, wl);
	w->nlen += wl - w->autolen;
	w->name[w->nlen] = 0;
	w->autolen = wl;
}

static void
paintdirty(int k)
{
	Wn *w = &wns[k];
	int base = BASE + STRIDE * k;
	char a[64];
	int on;

	on = w->stateseq != w->cleanseq;
	if(w->dirty != on){
		w->dirty = on;
		snprint(a, sizeof a, "bg=%s\n", on ? "#000000" : "#eaffff");
		nprint(base + 4, "attrs", a);
	}
	rebuildauto(k);
}

static void
setdirty(int k, int on)
{
	Wn *w = &wns[k];

	if(on)
		w->cleanseq = w->stateseq - 1;
	else
		w->cleanseq = w->stateseq;
	paintdirty(k);
}

/* ---- sam's language, the v0 core (Edit) ---- */

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
runedit(int k, char *line)
{
	Wn *w = &wns[k];
	static Span sp[512];
	static Resub subs[512 * 10];
	char re[256], arg[512], out[1024];
	Reprog *prog;
	char *p;
	int n, i, glob;
	ulong sq;

	sq = ++seq;
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
				change(k, sp[i].q0, sp[i].q1, arg, strlen(arg), sq);
		} else if(*p == 'd'){
			n = collect(w, prog, sp, 512, nil, 1);
			for(i = n - 1; i >= 0; i--)
				change(k, sp[i].q0, sp[i].q1, "", 0, sq);
		}
		free(prog);
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
		n = collect(w, prog, sp, 512, subs, 10);
		if(n > 0 && !glob)
			n = 1;
		for(i = n - 1; i >= 0; i--){
			regsub(arg, out, sizeof out, &subs[i * 10], 10);
			change(k, sp[i].q0, sp[i].q1, out, strlen(out), sq);
		}
		free(prog);
	}
}

static char *
afteredit(char *t, long n)
{
	static char c[CMAX];
	long i;
	char *p;

	t[n] = 0;
	p = strstr(t, "Edit");
	if(p == nil){
		c[0] = 0;
		return c;
	}
	p += 4;
	while(*p == ' ' || *p == '\t' || *p == '|')
		p++;
	for(i = 0; p[i] != 0 && p[i] != '\n' && i < CMAX - 1; i++)
		c[i] = p[i];
	c[i] = 0;
	return c;
}

/* ---- addresses, sam notation's core: n | n,m | #c | /re/ | $ | . ---- */

static long
linestart(Wn *w, long ln)
{
	long q, l;

	q = 0;
	l = 1;
	while(l < ln && q < w->blen){
		while(q < w->blen && w->body[q] != '\n')
			q++;
		if(q < w->blen)
			q++;
		l++;
	}
	return q;
}

static long
lineend(Wn *w, long q)
{
	while(q < w->blen && w->body[q] != '\n')
		q++;
	if(q < w->blen)
		q++;
	return q;
}

static char *
addr1(Wn *w, char *s, long *q0, long *q1)
{
	long n;
	char re[256];
	Reprog *prog;
	Resub m[1];
	long from;

	if(*s == '$'){
		*q0 = *q1 = w->blen;
		return s + 1;
	}
	if(*s == '.'){
		*q0 = w->sq0;
		*q1 = w->sq1;
		return s + 1;
	}
	if(*s == '#'){
		s++;
		n = 0;
		while(*s >= '0' && *s <= '9')
			n = n * 10 + (*s++ - '0');
		if(n > w->blen)
			n = w->blen;
		*q0 = *q1 = n;
		return s;
	}
	if(*s >= '0' && *s <= '9'){
		n = 0;
		while(*s >= '0' && *s <= '9')
			n = n * 10 + (*s++ - '0');
		if(n <= 0){
			*q0 = *q1 = 0;
			return s;
		}
		*q0 = linestart(w, n);
		*q1 = lineend(w, *q0);
		return s;
	}
	if(*s == '/'){
		s = delim(s, re, sizeof re);
		if(s == nil)
			return nil;
		prog = regcomp(re);
		if(prog == nil)
			return nil;
		w->body[w->blen] = 0;
		from = w->sq1 <= w->blen ? w->sq1 : 0;
		memset(m, 0, sizeof m);
		if(regexec(prog, w->body + from, m, 1)){
			*q0 = m[0].sp - w->body;
			*q1 = m[0].ep - w->body;
		} else {
			memset(m, 0, sizeof m);
			if(regexec(prog, w->body, m, 1)){	/* wrap once */
				*q0 = m[0].sp - w->body;
				*q1 = m[0].ep - w->body;
			} else {
				free(prog);
				return nil;
			}
		}
		free(prog);
		return s;
	}
	return nil;
}

/* parse a compound address into a range; 0 on failure */
static int
addrparse(Wn *w, char *s, long *q0, long *q1)
{
	long a0, a1, b0, b1;

	while(*s == ' ' || *s == '\t')
		s++;
	if(*s == 0){
		*q0 = 0;
		*q1 = w->blen;
		return 1;
	}
	if(*s == ','){
		s++;
		while(*s == ' ' || *s == '\t')
			s++;
		if(*s == 0 || *s == '$'){
			*q0 = 0;
			*q1 = w->blen;
			return 1;
		}
		s = addr1(w, s, &b0, &b1);
		if(s == nil)
			return 0;
		*q0 = 0;
		*q1 = b1;
		return 1;
	}
	s = addr1(w, s, &a0, &a1);
	if(s == nil)
		return 0;
	if(*s == ','){
		s++;
		if(*s == 0){
			*q0 = a0;
			*q1 = w->blen;
			return 1;
		}
		s = addr1(w, s, &b0, &b1);
		if(s == nil)
			return 0;
		*q0 = a0;
		*q1 = b1;
	} else {
		*q0 = a0;
		*q1 = a1;
	}
	if(*q1 < *q0)
		*q1 = *q0;
	return 1;
}

/* ---- windows and columns ---- */

static int
freecol(void)
{
	int c;

	for(c = 0; c < MAXCOL; c++)
		if(!cols[c].used)
			return c;
	return -1;
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

static int
rightcol(void)
{
	int c, r;

	r = active;
	for(c = 0; c < MAXCOL; c++)
		if(cols[c].used)
			r = c;
	return r;
}

static void
mkcol(int c)
{
	Col *col;
	int cbase;
	char b[256], a[128];

	col = &cols[c];
	cbase = CBASE + CSTRIDE * c;
	memset(col, 0, sizeof *col);
	col->used = 1;
	col->tlen = snprint(col->tag, sizeof col->tag, "New Delcol Sort | ");
	snprint(b, sizeof b, "new %d stack\nnew %d edit\n", cbase, cbase + 1);
	write(ctlfd, b, strlen(b));
	snprint(a, sizeof a, "parent=3\norder=%d\nprop=1\n", c + 1);
	nprint(cbase, "attrs", a);
	snprint(a, sizeof a, "parent=%d\nbg=#eaffff\norder=1\n", cbase);
	nprint(cbase + 1, "attrs", a);
	setnode(cbase + 1, col->tag, col->tlen);
	active = c;
}

static int
cmpdir(void *a, void *b)
{
	return strcmp(((Dir*)a)->name, ((Dir*)b)->name);
}

static long
listdir(char *path, char *buf, long max)
{
	Dir *d;
	int fd;
	long nd, i, o;

	fd = open(path, OREAD);
	if(fd < 0)
		return -1;
	nd = dirreadall(fd, &d);
	close(fd);
	if(nd < 0)
		return -1;
	qsort(d, nd, sizeof(Dir), cmpdir);
	o = 0;
	for(i = 0; i < nd; i++){
		long l = strlen(d[i].name);
		if(o + l + 2 >= max)
			break;
		memmove(buf + o, d[i].name, l);
		o += l;
		if(d[i].mode & DMDIR)
			buf[o++] = '/';
		buf[o++] = '\n';
	}
	buf[o] = 0;
	free(d);
	return o;
}

static void
mkwin(int k, char *file, int mode)
{
	Wn *w;
	int base, dird;
	char b[1024], a[256];
	long n;

	w = &wns[k];
	base = BASE + STRIDE * k;
	memset(w, 0, sizeof *w);
	w->used = 1;
	w->col = active;
	dird = mode == MKREAD && file != nil && isdir(file) == 1;
	if(dird)
		w->nlen = snprint(w->name, sizeof w->name, "%s%s Del Get",
			file, file[strlen(file) - 1] == '/' ? "" : "/");
	else
		w->nlen = snprint(w->name, sizeof w->name, "%s Del Get",
			file == nil ? "" : file);
	w->autopos = w->nlen;
	w->autolen = 0;
	w->nlen += snprint(w->name + w->nlen, sizeof w->name - w->nlen,
		dird ? " | " : " Edit | ");
	if(mode == MKREAD && file != nil){
		n = dird ? listdir(file, w->body, sizeof w->body)
			 : readfile(file, w->body, sizeof w->body);
		if(n >= 0)
			w->blen = n;
	}
	n = snprint(b, sizeof b,
		"new %d stack\nnew %d stack\nnew %d text\nnew %d edit\nnew %d edit\n",
		base, base + 3, base + 4, base + 1, base + 2);
	write(ctlfd, b, n);
	snprint(a, sizeof a, "parent=%d\norder=%d\n", CBASE + CSTRIDE * active, 2 + k);
	nprint(base, "attrs", a);
	snprint(a, sizeof a, "parent=%d\ndir=row\nbg=#eaffff\norder=1\n", base);
	nprint(base + 3, "attrs", a);
	snprint(a, sizeof a, "parent=%d\nbg=#eaffff\nprop=0\norder=1\n", base + 3);
	nprint(base + 4, "attrs", a);
	nprint(base + 4, "data", "  ");
	snprint(a, sizeof a, "parent=%d\nbg=#eaffff\norder=2\n", base + 3);
	nprint(base + 1, "attrs", a);
	snprint(a, sizeof a, "parent=%d\nbg=#ffffea\norder=2\n", base);
	nprint(base + 2, "attrs", a);
	setnode(base + 1, w->name, w->nlen);
	setnode(base + 2, w->body, w->blen);
}

static void
delwin(int k)
{
	Wn *w = &wns[k];

	fprint(ctlfd, "del %d\nsync\n", BASE + STRIDE * k);
	if(w->evpend && sfd >= 0){		/* release a parked event read */
		uchar *p = fsout + 7;
		put32(p, 0);			/* zero-length read: EOF */
		send9msg(sfd, Tread + 1, w->evtag, fsout, p + 4);
		w->evpend = 0;
	}
	free(w->ustk); free(w->rstk);
	w->ustk = w->rstk = nil;
	w->used = 0;
}

/* find (or make, towards the right — the paper's placement) dir/+Errors */
static int
errwin(char *dir)
{
	char name[NMAX], *fn;
	long fl;
	int k, oa;

	snprint(name, sizeof name, "%s/+Errors", strcmp(dir, "/") == 0 ? "" : dir);
	for(k = 0; k < MAXWIN; k++){
		if(!wns[k].used)
			continue;
		fn = firstword(wns[k].name, wns[k].nlen, &fl);
		if(strcmp(fn, name) == 0)
			return k;
	}
	k = freewin();
	if(k < 0)
		return -1;
	oa = active;
	active = rightcol();
	mkwin(k, name, MKEMPTY);
	active = oa;
	fprint(ctlfd, "sync\n");
	return k;
}

/* append to dir/+Errors; the selection rides the tail so the surface
 * scrolls with the output */
static void
errappend(char *dir, char *s, long n)
{
	int k;
	Wn *w;

	k = errwin(dir);
	if(k < 0)
		return;
	w = &wns[k];
	if(w->blen + n > BMAX - 1)
		n = BMAX - 1 - w->blen;
	if(n <= 0)
		return;
	noderepl(BASE + STRIDE * k + 2, w->blen, w->blen, s, n);
	memmove(w->body + w->blen, s, n);
	w->blen += n;
	w->body[w->blen] = 0;
	setsel(k, w->blen, w->blen);
	fprint(ctlfd, "sync\n");
}

/* ---- snarf: /dev/snarf IS the host clipboard (both ways) ---- */

static void
snarfput(char *s, long n)
{
	int fd;

	fd = open("#w/snarf", OWRITE | OTRUNC);
	if(fd < 0)
		fd = open("#w/snarf", OWRITE);
	if(fd >= 0){
		write(fd, s, n);
		close(fd);
	}
}

static long
snarfget(char *buf, long max)
{
	return readfile("#w/snarf", buf, max);
}

static void
unpost(void)
{
	if(sfd >= 0)
		remove("/srv/acme");
}

/* ---- external commands: the paper's mk workflow ---- */

static void
cmdchild(void *v)
{
	Cmd *c = v;

	rfork(RFFDG | RFNOWAIT);	/* intercepted by the port: stashed, restored */
	chdir(c->dir);
	dup(c->infd, 0);
	dup(c->outfd, 1);
	dup(c->outfd, 2);
	close(c->infd);
	close(c->outfd);
	close(c->pinw);			/* the inherited far ends: without these
	close(c->poutr);		 * closes stdin never sees EOF */
	procexec(pidc, c->xpath, c->av);
	threadexits("exec");
}

static void
outreader(void *v)
{
	Cmd *c = v;
	Msg *m;
	char buf[1024];
	long n;

	for(;;){
		n = read(c->outfd, buf, sizeof buf);
		m = malloc(sizeof(Msg));
		m->kind = n > 0 ? MOUT : MEOF;
		m->slot = c - cmds;
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
inwriter(void *v)
{
	Cmd *c = v;

	if(c->ilen > 0)
		write(c->infd, c->ibuf, c->ilen);
	close(c->infd);
	free(c->ibuf);
	c->ibuf = nil;
	threadexits(nil);
}

static void
runcmd(int k, char *word, char *args)
{
	Cmd *c;
	Wn *w;
	int i, slot, kind, pin[2], pout[2];
	char *p, dir[NMAX], try[NMAX];

	kind = CPLAIN;
	if(*word == '|'){ kind = CPIPE; word++; }
	else if(*word == '<'){ kind = CIN; word++; }
	else if(*word == '>'){ kind = COUT; word++; }
	if(*word == 0)
		return;
	if(kind != CPLAIN && k < 0)
		return;				/* the filters need a window */
	for(slot = 0; slot < MAXCMD && cmds[slot].used; slot++)
		;
	windirof(k, dir, sizeof dir);
	if(slot == MAXCMD){
		errappend(dir, "acme: too many commands\n", 24);
		return;
	}
	c = &cmds[slot];
	memset(c, 0, sizeof *c);
	c->used = 1;
	c->kind = kind;
	c->targetk = k;
	snprint(c->name, sizeof c->name, "%s", word);
	snprint(c->dir, sizeof c->dir, "%s", dir);
	if(word[0] == '/')
		snprint(c->xpath, sizeof c->xpath, "%s", word);
	else if(strchr(word, '/') != nil)
		snprint(c->xpath, sizeof c->xpath, "%s/%s", dir, word);
	else {
		snprint(try, sizeof try, "%s/%s", strcmp(dir, "/") == 0 ? "" : dir, word);
		if(isdir(try) == 0)		/* an existing plain file wins */
			snprint(c->xpath, sizeof c->xpath, "%s", try);
		else
			snprint(c->xpath, sizeof c->xpath, "/bin/%s", word);
	}
	/* argv: the word then the swept arguments */
	snprint(c->astore, sizeof c->astore, "%s", args == nil ? "" : args);
	c->av[0] = c->name;
	i = 1;
	p = c->astore;
	while(*p != 0 && i < 15){
		while(*p == ' ' || *p == '\t')
			*p++ = 0;
		if(*p == 0)
			break;
		c->av[i++] = p;
		while(*p != 0 && *p != ' ' && *p != '\t')
			p++;
	}
	c->av[i] = nil;
	if(pipe(pout) < 0){
		c->used = 0;
		return;
	}
	if(pipe(pin) < 0){
		close(pout[0]); close(pout[1]);
		c->used = 0;
		return;
	}
	if(kind == CPIPE || kind == COUT){	/* the selection feeds stdin */
		w = &wns[k];
		c->tq0 = w->sq0;
		c->tq1 = w->sq1;
		if(c->tq1 > w->blen) c->tq1 = w->blen;
		if(c->tq0 > c->tq1) c->tq0 = c->tq1;
		c->ilen = c->tq1 - c->tq0;
		c->ibuf = malloc(c->ilen + 1);
		memmove(c->ibuf, w->body + c->tq0, c->ilen);
	} else if(kind == CIN && k >= 0){
		w = &wns[k];
		c->tq0 = w->sq0;
		c->tq1 = w->sq1;
		if(c->tq1 > w->blen) c->tq1 = w->blen;
		if(c->tq0 > c->tq1) c->tq0 = c->tq1;
	}
	c->infd = pin[0];
	c->outfd = pout[1];
	c->pinw = pin[1];
	c->poutr = pout[0];
	proccreate(cmdchild, c, STACK);
	c->pid = recvul(pidc);
	close(pin[0]);
	close(pout[1]);
	c->outfd = pout[0];
	c->infd = pin[1];
	threadcreate(outreader, c, STACK);
	if(kind == CPIPE || kind == COUT)
		threadcreate(inwriter, c, STACK);
	else
		close(c->infd);
}

static void
cmddone(int slot)
{
	Cmd *c = &cmds[slot];
	Wn *w;
	int k;

	close(c->outfd);
	if((c->kind == CPIPE || c->kind == CIN) && c->targetk >= 0
	   && wns[c->targetk].used){
		k = c->targetk;
		w = &wns[k];
		if(c->tq1 > w->blen) c->tq1 = w->blen;
		if(c->tq0 > c->tq1) c->tq0 = c->tq1;
		change(k, c->tq0, c->tq1, c->obuf == nil ? "" : c->obuf, c->olen, ++seq);
		setnode(BASE + STRIDE * k + 2, w->body, w->blen);
		setsel(k, c->tq0, c->tq0 + c->olen);
		setdirty(k, 1);
		fprint(ctlfd, "sync\n");
	}
	free(c->obuf);
	c->obuf = nil;
	c->used = 0;
}

/* ---- the event file: delegation, the paper's "most unusual file" ---- */

static void
evsatisfy(int k)
{
	Wn *w = &wns[k];
	uchar *p;
	uint n;

	if(!w->evpend || w->evqlen == 0 || sfd < 0)
		return;
	n = w->evqlen;
	if(n > w->evcount)
		n = w->evcount;
	p = fsout + 7;
	put32(p, n);
	memmove(p + 4, w->evq, n);
	send9msg(sfd, Tread + 1, w->evtag, fsout, p + 4 + n);
	memmove(w->evq, w->evq + n, w->evqlen - n);
	w->evqlen -= n;
	w->evpend = 0;
}

static void
evqueue(int k, char origin, char type, long q0, long q1, char *text)
{
	Wn *w = &wns[k];
	char line[CMAX + 64];
	long n, tl;

	tl = text == nil ? 0 : strlen(text);
	if(tl > CMAX)
		tl = CMAX;
	n = snprint(line, sizeof line, "%c%c%ld %ld 0 %ld ", origin, type, q0, q1, tl);
	memmove(line + n, text == nil ? "" : text, tl);
	n += tl;
	line[n++] = '\n';
	if(w->evqlen + n <= EVMAX){
		memmove(w->evq + w->evqlen, line, n);
		w->evqlen += n;
	}
	evsatisfy(k);
}

/* ---- execute and look, the two verbs ---- */

static void
doexecute(int k, int c, char *word, char *args, int fromev)
{
	Wn *w;
	int base, k2, i;
	static char txt[BMAX];
	char *fn;
	long fl, n;
	int fd;

	USED(fromev);
	if(args == nil)
		args = "";
	/* the root tag's words */
	if(k < 0 && c < 0){
		if(strcmp(word, "Newcol") == 0){
			k2 = freecol();
			if(k2 >= 0){
				mkcol(k2);
				fprint(ctlfd, "sync\n");
			}
		} else if(strcmp(word, "Putall") == 0){
			for(k2 = 0; k2 < MAXWIN; k2++){
				if(!wns[k2].used)
					continue;
				fn = firstword(wns[k2].name, wns[k2].nlen, &fl);
				if(fl == 0 || fn[0] == '|' || fn[fl-1] == '/' || strstr(fn, "+Errors") != nil)
					continue;
				fd = create(fn, OWRITE, 0644);
				if(fd >= 0){
					write(fd, wns[k2].body, wns[k2].blen);
					close(fd);
					wns[k2].cleanseq = wns[k2].stateseq;
					paintdirty(k2);
				}
			}
			fprint(ctlfd, "sync\n");
		} else if(strcmp(word, "Dump") == 0){
			fd = create("/tmp/acme.dump", OWRITE, 0644);
			if(fd >= 0){
				for(k2 = 0; k2 < MAXWIN; k2++){
					if(!wns[k2].used)
						continue;
					fn = firstword(wns[k2].name, wns[k2].nlen, &fl);
					if(fl > 0 && fn[0] != '|')
						fprint(fd, "%s\n", fn);
				}
				close(fd);
			}
		} else if(strcmp(word, "Load") == 0){
			char db[2048], *dl[32];
			int nd;
			n = readfile("/tmp/acme.dump", db, sizeof db);
			if(n > 0){
				nd = getfields(db, dl, 32, 1, "\n");
				for(i = 0; i < nd; i++){
					if(dl[i][0] == 0)
						continue;
					k2 = freewin();
					if(k2 < 0)
						break;
					mkwin(k2, dl[i], MKREAD);
				}
				fprint(ctlfd, "sync\n");
			}
		} else if(strcmp(word, "Exit") == 0){
			for(i = 0; i < MAXCMD; i++)
				if(cmds[i].used)
					postnote(PNPROC, cmds[i].pid, "kill");
			unpost();
			threadexitsall(nil);
		} else if(strcmp(word, "Kill") == 0){
			for(i = 0; i < MAXCMD; i++)
				if(cmds[i].used && strcmp(cmds[i].name, args) == 0)
					postnote(PNPROC, cmds[i].pid, "kill");
		} else
			runcmd(-1, word, args);
		return;
	}

	/* a column's words */
	if(c >= 0){
		if(strcmp(word, "New") == 0){
			k2 = freewin();
			if(k2 >= 0){
				mkwin(k2, nil, MKREAD);
				fprint(ctlfd, "sync\n");
			}
		} else if(strcmp(word, "Delcol") == 0){
			for(k2 = 0; k2 < MAXWIN; k2++)
				if(wns[k2].used && wns[k2].col == c)
					delwin(k2);
			fprint(ctlfd, "del %d\nsync\n", CBASE + CSTRIDE * c);
			cols[c].used = 0;
			if(active == c)
				for(active = 0; active < MAXCOL && !cols[active].used; active++)
					;
		} else if(strcmp(word, "Sort") == 0){
			int ks[MAXWIN], nk, j, t;
			char a[64];
			nk = 0;
			for(k2 = 0; k2 < MAXWIN; k2++)
				if(wns[k2].used && wns[k2].col == c)
					ks[nk++] = k2;
			for(i = 0; i < nk; i++)
				for(j = i + 1; j < nk; j++){
					long l1, l2;
					char n1[NMAX], n2[NMAX];
					snprint(n1, sizeof n1, "%s", firstword(wns[ks[i]].name, wns[ks[i]].nlen, &l1));
					snprint(n2, sizeof n2, "%s", firstword(wns[ks[j]].name, wns[ks[j]].nlen, &l2));
					if(strcmp(n1, n2) > 0){
						t = ks[i]; ks[i] = ks[j]; ks[j] = t;
					}
				}
			for(i = 0; i < nk; i++){
				snprint(a, sizeof a, "order=%d\n", 2 + i);
				nprint(BASE + STRIDE * ks[i], "attrs", a);
			}
			fprint(ctlfd, "sync\n");
		} else
			runcmd(-1, word, args);
		return;
	}

	/* a window's words */
	w = &wns[k];
	base = BASE + STRIDE * k;
	if(strcmp(word, "Get") == 0){
		fn = firstword(w->name, w->nlen, &fl);
		n = fl > 0 && fn[fl-1] == '/'
			? listdir(fn, txt, sizeof txt)
			: readfile(fn, txt, sizeof txt);
		if(n >= 0){
			change(k, 0, w->blen, txt, n, ++seq);
			setnode(base + 2, w->body, w->blen);
			w->cleanseq = w->stateseq;
			paintdirty(k);
			fprint(ctlfd, "sync\n");
		}
	} else if(strcmp(word, "Put") == 0){
		fn = firstword(w->name, w->nlen, &fl);
		if(fl == 0)
			return;
		fd = create(fn, OWRITE, 0644);
		if(fd >= 0){
			write(fd, w->body, w->blen);
			close(fd);
			w->cleanseq = w->stateseq;
			paintdirty(k);
			fprint(ctlfd, "sync\n");
		}
	} else if(strcmp(word, "Del") == 0 || strcmp(word, "Delete") == 0){
		delwin(k);
	} else if(strcmp(word, "Zerox") == 0){
		k2 = freewin();
		if(k2 >= 0){
			fn = firstword(w->name, w->nlen, &fl);
			mkwin(k2, fl ? fn : nil, MKEMPTY);
			memmove(wns[k2].body, w->body, w->blen);
			wns[k2].blen = w->blen;
			setnode(BASE + STRIDE * k2 + 2, wns[k2].body, wns[k2].blen);
			fprint(ctlfd, "sync\n");
		}
	} else if(strcmp(word, "Edit") == 0){
		runedit(k, *args != 0 ? args : afteredit(w->name, w->nlen));
		setnode(base + 2, w->body, w->blen);
		setdirty(k, 1);
		fprint(ctlfd, "sync\n");
	} else if(strcmp(word, "Undo") == 0 || strcmp(word, "Redo") == 0){
		int redo = word[0] == 'R';
		uchar **from = redo ? &w->rstk : &w->ustk;
		long *flen = redo ? &w->rlen : &w->ulen;
		uchar **to = redo ? &w->ustk : &w->rstk;
		long *tlen = redo ? &w->ulen : &w->rlen;
		ulong sq = topseq(*from, *flen);
		long lq0 = 0, lql = 0;
		if(*flen == 0)
			return;
		while(*flen > 0 && topseq(*from, *flen) == sq){
			long rl = get4(*from + *flen - 4);
			uchar *p = *from + *flen - rl;
			long q0 = get4(p), al = get4(p + 4), bl = get4(p + 8);
			static char sa[BMAX], sb[BMAX];
			memmove(sa, p + 16, al);
			memmove(sb, p + 16 + al, bl);
			if(redo)
				splice(w, q0, q0 + al, sb, bl);
			else
				splice(w, q0, q0 + bl, sa, al);
			*flen -= rl;
			pushrec(to, tlen, q0, sa, al, sb, bl, sq);
			lq0 = q0;
			lql = redo ? bl : al;
		}
		w->stateseq = topseq(w->ustk, w->ulen);
		lastinsk = -1;
		setnode(base + 2, w->body, w->blen);
		setsel(k, lq0, lq0 + lql);
		paintdirty(k);
		fprint(ctlfd, "sync\n");
	} else if(strcmp(word, "Cut") == 0 || strcmp(word, "Snarf") == 0){
		long q0 = w->sq0, q1 = w->sq1;
		if(q1 > w->blen) q1 = w->blen;
		if(q0 > q1) q0 = q1;
		if(q1 > q0)
			snarfput(w->body + q0, q1 - q0);
		if(word[0] == 'C' && q1 > q0){
			change(k, q0, q1, "", 0, ++seq);
			setnode(base + 2, w->body, w->blen);
			setsel(k, q0, q0);
			setdirty(k, 1);
			fprint(ctlfd, "sync\n");
		}
	} else if(strcmp(word, "Paste") == 0){
		long q0 = w->sq0, q1 = w->sq1;
		n = snarfget(txt, sizeof txt);
		if(n < 0)
			return;
		if(q1 > w->blen) q1 = w->blen;
		if(q0 > q1) q0 = q1;
		change(k, q0, q1, txt, n, ++seq);
		setnode(base + 2, w->body, w->blen);
		setsel(k, q0, q0 + n);
		setdirty(k, 1);
		fprint(ctlfd, "sync\n");
	} else if(strcmp(word, "Look") == 0){
		if(*args != 0){
			char *hit;
			long al = strlen(args);
			w->body[w->blen] = 0;
			hit = w->sq1 < w->blen ? strstr(w->body + w->sq1, args) : nil;
			if(hit == nil)
				hit = strstr(w->body, args);
			if(hit != nil){
				setsel(k, hit - w->body, hit - w->body + al);
				fprint(ctlfd, "sync\n");
			}
		}
	} else if(strcmp(word, "ID") == 0){
		char dir[NMAX], idl[32];
		windirof(k, dir, sizeof dir);
		snprint(idl, sizeof idl, "%d\n", base);
		errappend(dir, idl, strlen(idl));
	} else if(strcmp(word, "Kill") == 0){
		for(i = 0; i < MAXCMD; i++)
			if(cmds[i].used && strcmp(cmds[i].name, args) == 0)
				postnote(PNPROC, cmds[i].pid, "kill");
	} else
		runcmd(k, word, args);
}

/* B3: names open — files, dirs, name:addr, :addr context searches and
 * <bracketed> headers; a window already holding the file is reused (the
 * paper's rule). Literal search stays the presenter's half. */
static void
dolook(int k, char *text, int fromev)
{
	static char resolved[512];
	char *target, *colon, addr[128];
	int k2, hasaddr, j;
	Wn *w;
	long fl;
	char *fn;

	USED(fromev);
	hasaddr = 0;
	addr[0] = 0;
	target = text;
	if(text[0] == '<' && text[strlen(text) - 1] == '>'){
		text[strlen(text) - 1] = 0;
		snprint(resolved, sizeof resolved, "/sys/include/%s", text + 1);
		if(isdir(resolved) < 0)
			return;
		target = resolved;
	} else {
		colon = strrchr(text, ':');
		if(colon != nil && (colon[1] == '/' || colon[1] == '$' || colon[1] == '#'
		   || (colon[1] >= '0' && colon[1] <= '9'))){
			snprint(addr, sizeof addr, "%s", colon + 1);
			*colon = 0;
			hasaddr = 1;
		}
		if(text[0] == 0 && hasaddr && k >= 0){
			long q0, q1;
			w = &wns[k];
			if(addrparse(w, addr, &q0, &q1)){
				setsel(k, q0, q1);
				fprint(ctlfd, "sync\n");
			}
			return;
		}
		if(text[0] == 0)
			return;
		if(text[0] != '/' && k >= 0 && wns[k].used){
			char dbase[NMAX];
			windirof(k, dbase, sizeof dbase);
			snprint(resolved, sizeof resolved, "%s/%s",
				strcmp(dbase, "/") == 0 ? "" : dbase, text);
			target = resolved;
		}
	}
	if(isdir(target) < 0)
		return;
	k2 = -1;
	{
		char tname[NMAX];
		snprint(tname, sizeof tname, "%s%s", target,
			isdir(target) == 1 && target[strlen(target)-1] != '/' ? "/" : "");
		for(j = 0; j < MAXWIN; j++){
			if(!wns[j].used)
				continue;
			fn = firstword(wns[j].name, wns[j].nlen, &fl);
			if(strcmp(fn, tname) == 0){
				k2 = j;
				break;
			}
		}
	}
	if(k2 < 0){
		k2 = freewin();
		if(k2 < 0)
			return;
		mkwin(k2, target, MKREAD);
	}
	if(hasaddr){
		long q0, q1;
		w = &wns[k2];
		if(addrparse(w, addr, &q0, &q1))
			setsel(k2, q0, q1);
	}
	fprint(ctlfd, "sync\n");
}

/* ---- the 9P server: index, new, N/{tag,body,addr,data,ctl,event} ---- */

static int
findfid(int fid, int alloc)
{
	int i, fr;

	fr = -1;
	for(i = 0; i < NFID; i++){
		if(fids[i] == fid)
			return i;
		if(fids[i] == -1 && fr < 0)
			fr = i;
	}
	if(alloc && fr >= 0){
		fids[fr] = fid;
		fidopen[fr] = 0;
		return fr;
	}
	return -1;
}

static int
nodewin(int node)
{
	if(node < WNODE)
		return -1;
	return (node - WNODE) / WSTRIDE;
}

static int
nodefile(int node)
{
	return (node - WNODE) % WSTRIDE;
}

static int
isdirnode9(int node)
{
	return node == 0 || node == NNEW || (node >= WNODE && nodefile(node) == 0);
}

static uchar *
pqid9(uchar *p, int node)
{
	return putqid(p, isdirnode9(node) ? QTDIR9 : QTFILE9, node + 500);
}

static uchar *
pstat9(uchar *p, char *name, int node)
{
	uchar *sz = p;

	p = put16(p, 0);
	p = put16(p, 0); p = put32(p, 0);
	p = pqid9(p, node);
	p = put32(p, isdirnode9(node) ? (0x80000000 | 0555) : 0666);
	p = put32(p, 0); p = put32(p, 0);
	p = put64(p, 0);
	p = putstr(p, name);
	p = putstr(p, "acme"); p = putstr(p, "acme"); p = putstr(p, "acme");
	put16(sz, p - sz - 2);
	return p;
}

static char *wfiles[] = { "", "tag", "body", "addr", "data", "ctl", "event" };

static long
ctlline(int k, char *buf, long max)
{
	Wn *w = &wns[k];
	char *fn;
	long fl;
	int dirw;

	fn = firstword(w->name, w->nlen, &fl);
	dirw = fl > 0 && fn[fl-1] == '/';
	return snprint(buf, max, "%11d %11ld %11ld %11d %11d %.*s\n",
		BASE + STRIDE * k, w->nlen, w->blen, dirw, w->dirty,
		(int)w->nlen, w->name);
}

static char *
ctl9(int k, char *line)
{
	Wn *w = &wns[k];
	long q0, q1;

	if(strncmp(line, "name ", 5) == 0){
		char *nn = line + 5;
		long fl, nl = strlen(nn);
		firstword(w->name, w->nlen, &fl);
		if(nl + w->nlen - fl > NMAX - 1)
			return "name too long";
		noderepl(BASE + STRIDE * k + 1, 0, fl, nn, nl);
		memmove(w->name + nl, w->name + fl, w->nlen - fl);
		memmove(w->name, nn, nl);
		w->nlen += nl - fl;
		w->name[w->nlen] = 0;
		w->autopos += nl - fl;
		fprint(ctlfd, "sync\n");
		return nil;
	}
	if(strcmp(line, "clean") == 0){
		w->cleanseq = w->stateseq;
		paintdirty(k);
		fprint(ctlfd, "sync\n");
		return nil;
	}
	if(strcmp(line, "dirty") == 0){
		setdirty(k, 1);
		fprint(ctlfd, "sync\n");
		return nil;
	}
	if(strcmp(line, "del") == 0 || strcmp(line, "delete") == 0){
		delwin(k);
		return nil;
	}
	if(strcmp(line, "get") == 0){
		doexecute(k, -1, "Get", "", 1);
		return nil;
	}
	if(strcmp(line, "put") == 0){
		doexecute(k, -1, "Put", "", 1);
		return nil;
	}
	if(strcmp(line, "show") == 0){
		setsel(k, w->wq0, w->wq1);
		fprint(ctlfd, "sync\n");
		return nil;
	}
	if(strcmp(line, "addr=dot") == 0){
		w->wq0 = w->sq0;
		w->wq1 = w->sq1;
		return nil;
	}
	if(strcmp(line, "dot=addr") == 0){
		setsel(k, w->wq0, w->wq1);
		fprint(ctlfd, "sync\n");
		return nil;
	}
	if(addrparse(w, line, &q0, &q1)){	/* a bare address line */
		w->wq0 = q0;
		w->wq1 = q1;
		return nil;
	}
	return "unknown ctl";
}

static void
do9p(uchar *msg, long rn)
{
	int type, tag, fid, nf, node, i, k;
	uchar *p, *b;
	uint count, got;
	uvlong off;
	static char sbuf[BMAX];
	long n;
	Wn *w;

	USED(rn);
	type = msg[4];
	tag = get16(msg + 5);
	b = msg + 7;
	p = fsout + 7;
	switch(type){
	case Tversion:
		count = get32(b);
		p = put32(p, count < MSIZE9 ? count : MSIZE9);
		p = putstr(p, "9P2000");
		send9msg(sfd, type + 1, tag, fsout, p);
		break;
	case Tattach:
		fid = get32(b);
		nf = findfid(fid, 1);
		if(nf < 0){ send9err(sfd, tag, "out of fids", fsout); break; }
		fidnode[nf] = 0;
		p = pqid9(p, 0);
		send9msg(sfd, type + 1, tag, fsout, p);
		break;
	case Twalk: {
		int newfid = get32(b + 4), nname = get16(b + 8);
		char name[64];
		uint ln;

		nf = findfid(fid = get32(b), 0);
		if(nf < 0){ send9err(sfd, tag, "unknown fid", fsout); break; }
		node = fidnode[nf];
		if(nname == 0){
			nf = findfid(newfid, 1);
			if(nf < 0){ send9err(sfd, tag, "out of fids", fsout); break; }
			fidnode[nf] = node;
			p = put16(p, 0);
			send9msg(sfd, type + 1, tag, fsout, p);
			break;
		}
		if(nname != 1){ send9err(sfd, tag, "one name per walk here", fsout); break; }
		ln = get16(b + 10);
		if(ln > 63){ send9err(sfd, tag, "name too long", fsout); break; }
		memcpy(name, b + 12, ln);
		name[ln] = 0;
		if(node == 0){
			if(strcmp(name, "index") == 0)
				node = NINDEX;
			else if(strcmp(name, "new") == 0)
				node = NNEW;
			else {
				int wbase = atoi(name), k2 = (wbase - BASE) / STRIDE;
				if(wbase >= BASE && k2 >= 0 && k2 < MAXWIN
				   && wbase == BASE + STRIDE * k2 && wns[k2].used)
					node = WNODE + WSTRIDE * k2;
				else { send9err(sfd, tag, "file not found", fsout); break; }
			}
		} else if(node == NNEW){
			for(i = 1; i <= 6; i++)
				if(strcmp(name, wfiles[i]) == 0)
					break;
			if(i > 6){ send9err(sfd, tag, "file not found", fsout); break; }
			k = freewin();		/* the clone pattern: a walk mints */
			if(k < 0){ send9err(sfd, tag, "window table full", fsout); break; }
			mkwin(k, nil, MKREAD);
			fprint(ctlfd, "sync\n");
			node = WNODE + WSTRIDE * k + i;
		} else if(node >= WNODE && nodefile(node) == 0){
			for(i = 1; i <= 6; i++)
				if(strcmp(name, wfiles[i]) == 0)
					break;
			if(i > 6){ send9err(sfd, tag, "file not found", fsout); break; }
			node += i;
		} else { send9err(sfd, tag, "file not found", fsout); break; }
		nf = findfid(newfid, 1);
		if(nf < 0){ send9err(sfd, tag, "out of fids", fsout); break; }
		fidnode[nf] = node;
		p = put16(p, 1);
		p = pqid9(p, node);
		send9msg(sfd, type + 1, tag, fsout, p);
		break;
	}
	case Topen:
		nf = findfid(get32(b), 0);
		if(nf < 0){ send9err(sfd, tag, "unknown fid", fsout); break; }
		node = fidnode[nf];
		fidopen[nf] = 1;
		if(node >= WNODE && nodefile(node) == FEVENT){
			k = nodewin(node);
			if(k >= 0 && k < MAXWIN && wns[k].used)
				wns[k].evopen++;
		}
		p = pqid9(p, node);
		p = put32(p, 0);
		send9msg(sfd, type + 1, tag, fsout, p);
		break;
	case Tread: {
		uchar dbuf[2048], *d, *e2;

		nf = findfid(get32(b), 0);
		if(nf < 0){ send9err(sfd, tag, "unknown fid", fsout); break; }
		node = fidnode[nf];
		off = get64(b + 4);
		count = get32(b + 12);
		if(count > MSIZE9 - 24)
			count = MSIZE9 - 24;
		if(node == 0 || node == NNEW || (node >= WNODE && nodefile(node) == 0)){
			d = dbuf;
			if(node == 0){
				d = pstat9(d, "index", NINDEX);
				d = pstat9(d, "new", NNEW);
				for(k = 0; k < MAXWIN; k++)
					if(wns[k].used){
						char nm[16];
						snprint(nm, sizeof nm, "%d", BASE + STRIDE * k);
						d = pstat9(d, nm, WNODE + WSTRIDE * k);
					}
			} else {
				int wb = node == NNEW ? 0 : node;
				for(i = 1; i <= 6; i++)
					d = pstat9(d, wfiles[i], wb + i);
			}
			e2 = d;
			got = 0;
			d = dbuf;
			while(d < e2 && (uvlong)(d - dbuf) < off)
				d += get16(d) + 2;
			while(d < e2 && d + get16(d) + 2 <= e2 && got + get16(d) + 2 <= count){
				memcpy(p + 4 + got, d, get16(d) + 2);
				got += get16(d) + 2;
				d += get16(d) + 2;
			}
			put32(p, got);
			p += 4 + got;
			send9msg(sfd, type + 1, tag, fsout, p);
			break;
		}
		if(node == NINDEX){
			n = 0;
			for(i = 0; i < MAXWIN; i++)
				if(wns[i].used)
					n += ctlline(i, sbuf + n, sizeof sbuf - n);
		} else {
			k = nodewin(node);
			if(k < 0 || k >= MAXWIN || !wns[k].used){
				send9err(sfd, tag, "window gone", fsout);
				break;
			}
			w = &wns[k];
			n = 0;
			switch(nodefile(node)){
			case FTAG:
				n = w->nlen;
				memmove(sbuf, w->name, n);
				break;
			case FBODY:
				n = w->blen;
				memmove(sbuf, w->body, n);
				break;
			case FADDR:
				n = snprint(sbuf, sizeof sbuf, "%11ld %11ld ", w->wq0, w->wq1);
				break;
			case FDATA: {
				long dq0 = w->wq0, dq1 = w->wq1;
				if(dq1 > w->blen) dq1 = w->blen;
				if(dq0 > dq1) dq0 = dq1;
				n = dq1 - dq0;
				memmove(sbuf, w->body + dq0, n);
				break;
			}
			case FCTL:
				n = ctlline(k, sbuf, sizeof sbuf);
				break;
			case FEVENT:
				if(w->evqlen == 0){	/* park until an event lands */
					w->evpend = 1;
					w->evtag = tag;
					w->evcount = count;
					return;
				}
				n = w->evqlen;
				if((uint)n > count)
					n = count;
				memmove(sbuf, w->evq, n);
				memmove(w->evq, w->evq + n, w->evqlen - n);
				w->evqlen -= n;
				put32(p, n);
				memcpy(p + 4, sbuf, n);
				p += 4 + n;
				send9msg(sfd, type + 1, tag, fsout, p);
				goto readout;
			}
		}
		if(off > (uvlong)n)
			off = n;
		got = n - off;
		if(got > count)
			got = count;
		put32(p, got);
		memcpy(p + 4, sbuf + off, got);
		p += 4 + got;
		send9msg(sfd, type + 1, tag, fsout, p);
	readout:
		break;
	}
	case Twrite: {
		static char pay[BMAX];

		nf = findfid(get32(b), 0);
		if(nf < 0){ send9err(sfd, tag, "unknown fid", fsout); break; }
		node = fidnode[nf];
		count = get32(b + 12);
		if(count > sizeof pay - 1)
			count = sizeof pay - 1;
		memcpy(pay, b + 16, count);
		pay[count] = 0;
		k = nodewin(node);
		if(node < WNODE || k < 0 || k >= MAXWIN || !wns[k].used){
			send9err(sfd, tag, "read-only file", fsout);
			break;
		}
		w = &wns[k];
		switch(nodefile(node)){
		case FBODY:			/* writes append, the real semantic */
			change(k, w->blen, w->blen, pay, count, ++seq);
			setnode(BASE + STRIDE * k + 2, w->body, w->blen);
			setdirty(k, 1);
			fprint(ctlfd, "sync\n");
			break;
		case FTAG:
			if(w->nlen + (long)count > NMAX - 1){
				send9err(sfd, tag, "tag full", fsout);
				goto wrdone;
			}
			noderepl(BASE + STRIDE * k + 1, w->nlen, w->nlen, pay, count);
			memmove(w->name + w->nlen, pay, count);
			w->nlen += count;
			w->name[w->nlen] = 0;
			fprint(ctlfd, "sync\n");
			break;
		case FADDR: {
			long q0, q1;
			if(!addrparse(w, pay, &q0, &q1)){
				send9err(sfd, tag, "bad address", fsout);
				goto wrdone;
			}
			w->wq0 = q0;
			w->wq1 = q1;
			break;
		}
		case FDATA: {
			long dq0 = w->wq0, dq1 = w->wq1;
			if(dq1 > w->blen) dq1 = w->blen;
			if(dq0 > dq1) dq0 = dq1;
			change(k, dq0, dq1, pay, count, ++seq);
			w->wq0 = w->wq1 = dq0 + count;	/* further writes extend */
			setnode(BASE + STRIDE * k + 2, w->body, w->blen);
			setdirty(k, 1);
			fprint(ctlfd, "sync\n");
			break;
		}
		case FCTL: {
			char *ln2, *nl2, *err;
			err = nil;
			ln2 = pay;
			while(ln2 != nil && *ln2 != 0 && err == nil){
				nl2 = strchr(ln2, '\n');
				if(nl2 != nil)
					*nl2 = 0;
				if(*ln2 != 0)
					err = ctl9(k, ln2);
				ln2 = nl2 == nil ? nil : nl2 + 1;
			}
			if(err != nil){
				send9err(sfd, tag, err, fsout);
				goto wrdone;
			}
			break;
		}
		case FEVENT: {
			char o, t, txt2[CMAX];
			long q0, q1;
			if(count >= 2){
				o = pay[0]; t = pay[1];
				USED(o);
				q0 = strtol(pay + 2, nil, 10);
				{
					char *sp2 = strchr(pay + 2, ' ');
					q1 = sp2 != nil ? strtol(sp2 + 1, nil, 10) : q0;
				}
				if(t == 'X' || t == 'x' || t == 'L' || t == 'l'){
					char *src = (t == 'X' || t == 'L') ? w->body : w->name;
					long sl = (t == 'X' || t == 'L') ? w->blen : w->nlen;
					long tn;
					if(q0 < 0) q0 = 0;
					if(q1 > sl) q1 = sl;
					if(q0 > q1) q0 = q1;
					tn = q1 - q0;
					if(tn > (long)sizeof txt2 - 1)
						tn = sizeof txt2 - 1;
					memmove(txt2, src + q0, tn);
					txt2[tn] = 0;
					if(t == 'X' || t == 'x'){
						char *arg = strchr(txt2, ' ');
						if(arg != nil)
							*arg++ = 0;
						doexecute(k, -1, txt2, arg, 1);
					} else
						dolook(k, txt2, 1);
				}
			}
			break;
		}
		default:
			send9err(sfd, tag, "read-only file", fsout);
			goto wrdone;
		}
		p = put32(p, count);
		send9msg(sfd, type + 1, tag, fsout, p);
	wrdone:
		break;
	}
	case Tclunk:
	case Tremove:
		nf = findfid(get32(b), 0);
		if(nf >= 0){
			node = fidnode[nf];
			if(fidopen[nf] && node >= WNODE && nodefile(node) == FEVENT){
				k = nodewin(node);
				if(k >= 0 && k < MAXWIN && wns[k].used && wns[k].evopen > 0){
					wns[k].evopen--;
					if(wns[k].evopen == 0)
						wns[k].evpend = 0;
				}
			}
			fids[nf] = -1;
		}
		if(type == Tremove){ send9err(sfd, tag, "not removable", fsout); break; }
		send9msg(sfd, type + 1, tag, fsout, p);
		break;
	case Tstat: {
		uchar *sz;
		char nm[32];

		nf = findfid(get32(b), 0);
		if(nf < 0){ send9err(sfd, tag, "unknown fid", fsout); break; }
		node = fidnode[nf];
		if(node == 0)
			snprint(nm, sizeof nm, "/");
		else if(node == NINDEX)
			snprint(nm, sizeof nm, "index");
		else if(node == NNEW)
			snprint(nm, sizeof nm, "new");
		else if(nodefile(node) == 0)
			snprint(nm, sizeof nm, "%d", BASE + STRIDE * nodewin(node));
		else
			snprint(nm, sizeof nm, "%s", wfiles[nodefile(node)]);
		sz = p;
		p = put16(p, 0);
		p = pstat9(p, nm, node);
		put16(sz, p - sz - 2);
		send9msg(sfd, type + 1, tag, fsout, p);
		break;
	}
	case 108: {			/* Tflush(oldtag): let a parked read go */
		int oldtag = get16(b);
		for(i = 0; i < MAXWIN; i++)
			if(wns[i].used && wns[i].evpend && wns[i].evtag == oldtag){
				uchar *zp = fsout + 7;
				put32(zp, 0);
				send9msg(sfd, Tread + 1, oldtag, fsout, zp + 4);
				wns[i].evpend = 0;
			}
		p = fsout + 7;
		send9msg(sfd, 109, tag, fsout, p);
		break;
	}
	default:
		send9err(sfd, tag, "acme: unsupported request", fsout);
	}
}

/* ---- reader threads ---- */

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
		m->kind = MEV;
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
fsproc(void *v)
{
	Msg *m;
	long n;

	USED(v);
	for(;;){
		n = read9msg(sfd, fsmsg);
		if(n <= 0)
			threadexits(nil);
		m = malloc(sizeof(Msg));
		m->kind = MFS;
		m->n = n;
		m->buf = malloc(n);
		memmove(m->buf, fsmsg, n);
		sendp(mc, m);
	}
}

/* ---- the consumer: one thread owns every buffer ---- */

void
threadmain(int argc, char *argv[])
{
	static char txt[4096], fulltxt[4096];
	char path[128], *f[8], *word, *args;
	int wid, fd, nf, i, k, off, minted, pfd[2], postfd;
	long n, q0, q1, d;
	Wn *w;
	Msg *m;

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
	if(getwd(aroot, sizeof aroot) == nil)
		snprint(aroot, sizeof aroot, "/");

	snprint(path, sizeof path, "%s/canvas/ctl", wdir);
	ctlfd = open(path, OWRITE);
	if(ctlfd < 0)
		sysfatal("no canvas on this host: %r");

	fprint(ctlfd, "new 1 edit\nnew 3 stack\n");
	nprint(1, "attrs", "bg=#eaffff\norder=1\n");
	nprint(1, "data", "Newcol Putall Dump Load Exit | ");
	nprint(3, "attrs", "order=2\ndir=row\n");
	mkcol(0);

	k = 0;
	for(; i < argc && k < MAXWIN; i++, k++)
		mkwin(k, argv[i], MKREAD);
	if(k == 0){
		static char cwd[256];
		mkcol(1);
		if(getwd(cwd, sizeof cwd) != nil)
			mkwin(k++, cwd, MKREAD);
		else
			mkwin(k++, "/", MKREAD);
	}
	fprint(ctlfd, "sync\n");

	snprint(path, sizeof path, "%s/label", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "acme"); close(fd); }

	snprint(path, sizeof path, "%s/canvas/events", wdir);
	evfd = open(path, OREAD);
	if(evfd < 0)
		sysfatal("canvas events: %r");

	mc = chancreate(sizeof(Msg*), 16);
	pidc = chancreate(sizeof(ulong), 1);
	threadcreate(evproc, nil, STACK);

	/* the file interface, posted at /srv/acme (a second acme skips) */
	for(i = 0; i < NFID; i++)
		fids[i] = -1;
	if(pipe(pfd) >= 0){
		postfd = create("/srv/acme", OWRITE, 0600);
		if(postfd >= 0){
			fprint(postfd, "%d", pfd[0]);
			close(postfd);
			close(pfd[0]);
			sfd = pfd[1];
			threadcreate(fsproc, nil, STACK);
		} else {
			close(pfd[0]);
			close(pfd[1]);
		}
	}

	for(;;){
		m = recvp(mc);
		switch(m->kind){
		case MFS:
			do9p((uchar*)m->buf, m->n);
			break;
		case MOUT: {
			Cmd *cm = &cmds[m->slot];
			if(cm->kind == CPIPE || cm->kind == CIN){
				if(cm->obuf == nil)
					cm->obuf = malloc(BMAX);
				if(cm->olen + m->n < BMAX - 1){
					memmove(cm->obuf + cm->olen, m->buf, m->n);
					cm->olen += m->n;
				}
			} else
				errappend(cm->dir, m->buf, m->n);
			break;
		}
		case MEOF:
			cmddone(m->slot);
			break;
		case MEV: {
			if(m->n <= 0){
				unpost();
				threadexitsall(nil);
			}
			nf = tokenize(m->buf, f, 8);
			if(nf < 2)
				break;
			if(strcmp(f[0], "close") == 0){
				for(i = 0; i < MAXCMD; i++)
					if(cmds[i].used)
						postnote(PNPROC, cmds[i].pid, "kill");
				unpost();
				if(minted){
					snprint(path, sizeof path, "%s/wctl", wdir);
					fd = open(path, OWRITE);
					if(fd >= 0){ fprint(fd, "delete"); close(fd); }
				}
				threadexitsall(nil);
			}
			i = atoi(f[1]);
			word = nil;
			args = nil;
			if(nf >= 5 && strcmp(f[0], "execute") == 0){
				unq(f[4], txt, sizeof txt);
				snprint(fulltxt, sizeof fulltxt, "%s", txt);
				word = txt;
				args = txt;
				while(*args != 0 && *args != ' ' && *args != '\t')
					args++;
				if(*args != 0)
					*args++ = 0;
				while(*args == ' ' || *args == '\t')
					args++;
			}
			if(i == 1){			/* the root tag */
				if(word != nil)
					doexecute(-1, -1, word, args, 0);
				break;
			}
			if(i >= CBASE){			/* a column's tag */
				k = (i - CBASE) / CSTRIDE;
				if(k >= MAXCOL || !cols[k].used)
					break;
				active = k;
				if(nf >= 4 && strcmp(f[0], "insert") == 0){
					Col *cl = &cols[k];
					q0 = atol(f[2]);
					d = unq(f[3], txt, sizeof txt);
					if(q0 > cl->tlen) q0 = cl->tlen;
					if(cl->tlen + d > NMAX - 1) d = NMAX - 1 - cl->tlen;
					memmove(cl->tag + q0 + d, cl->tag + q0, cl->tlen - q0);
					memmove(cl->tag + q0, txt, d);
					cl->tlen += d;
					cl->tag[cl->tlen] = 0;
					break;
				}
				if(nf >= 4 && strcmp(f[0], "delete") == 0){
					Col *cl = &cols[k];
					q0 = atol(f[2]);
					q1 = atol(f[3]);
					if(q1 > cl->tlen) q1 = cl->tlen;
					if(q0 > q1) q0 = q1;
					memmove(cl->tag + q0, cl->tag + q1, cl->tlen - q1);
					cl->tlen -= q1 - q0;
					cl->tag[cl->tlen] = 0;
					break;
				}
				if(word != nil)
					doexecute(-1, k, word, args, 0);
				break;
			}
			if(i < BASE)
				break;
			k = (i - BASE) / STRIDE;
			off = (i - BASE) % STRIDE;
			if(k >= MAXWIN || !wns[k].used)
				break;
			w = &wns[k];
			active = w->col;
			if(nf >= 4 && strcmp(f[0], "select") == 0 && off == 2){
				w->sq0 = atol(f[2]);
				w->sq1 = atol(f[3]);
				if(w->sq0 > w->blen) w->sq0 = w->blen;
				if(w->sq1 > w->blen) w->sq1 = w->blen;
				break;
			}
			if(nf >= 4 && strcmp(f[0], "insert") == 0 && (off == 1 || off == 2)){
				char *buf = off == 1 ? w->name : w->body;
				long *len = off == 1 ? &w->nlen : &w->blen;
				long max = off == 1 ? NMAX : BMAX;
				q0 = atol(f[2]);
				d = unq(f[3], txt, sizeof txt);
				if(q0 > *len) q0 = *len;
				if(*len + d > max - 1) d = max - 1 - *len;
				memmove(buf + q0 + d, buf + q0, *len - q0);
				memmove(buf + q0, txt, d);
				*len += d;
				buf[*len] = 0;
				if(off == 1){		/* keep the auto block placed */
					if(q0 <= w->autopos)
						w->autopos += d;
					else if(q0 < w->autopos + w->autolen)
						w->autolen += d;
				} else {
					if(!(lastinsk == k && extendtop(w, q0, txt, d))){
						pushrec(&w->ustk, &w->ulen, q0, "", 0, txt, d, ++seq);
						w->rlen = 0;
						w->stateseq = seq;
					}
					lastinsk = k;
					if(w->evopen)
						evqueue(k, 'K', 'I', q0, q0 + d, nil);
					setdirty(k, 1);
					fprint(ctlfd, "sync\n");
				}
				break;
			}
			if(nf >= 4 && strcmp(f[0], "delete") == 0 && (off == 1 || off == 2)){
				char *buf = off == 1 ? w->name : w->body;
				long *len = off == 1 ? &w->nlen : &w->blen;
				q0 = atol(f[2]);
				q1 = atol(f[3]);
				if(q1 > *len) q1 = *len;
				if(q0 > q1) q0 = q1;
				if(off == 2){
					static char old[BMAX];
					memmove(old, buf + q0, q1 - q0);
					pushrec(&w->ustk, &w->ulen, q0, old, q1 - q0, "", 0, ++seq);
					w->rlen = 0;
					w->stateseq = seq;
					lastinsk = -1;
					if(w->evopen)
						evqueue(k, 'K', 'D', q0, q1, nil);
				}
				memmove(buf + q0, buf + q1, *len - q1);
				*len -= q1 - q0;
				buf[*len] = 0;
				if(off == 1){		/* interval math for the block */
					long b0 = w->autopos, b1 = w->autopos + w->autolen;
					long lo = q0 < b0 ? (q1 < b0 ? q1 : b0) - q0 : 0;
					long in = 0;
					if(q1 > b0 && q0 < b1){
						long s = q0 > b0 ? q0 : b0;
						long e = q1 < b1 ? q1 : b1;
						in = e - s;
					}
					w->autopos -= lo;
					w->autolen -= in;
				} else {
					setdirty(k, 1);
					fprint(ctlfd, "sync\n");
				}
				break;
			}
			if(nf >= 5 && strcmp(f[0], "look") == 0){
				unq(f[4], txt, sizeof txt);
				if(w->evopen)
					evqueue(k, 'M', off == 2 ? 'L' : 'l',
						atol(f[2]), atol(f[3]), txt);
				else
					dolook(k, txt, 0);
				break;
			}
			if(word == nil || (off != 1 && off != 2))
				break;
			if(w->evopen){
				evqueue(k, 'M', off == 2 ? 'X' : 'x',
					atol(f[2]), atol(f[3]), fulltxt);
				break;
			}
			doexecute(k, -1, word, args, 0);
			break;
		}
		}
		free(m->buf);
		free(m);
	}
}
