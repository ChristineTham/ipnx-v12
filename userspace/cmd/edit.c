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
	MAXCOL = 4,
	BMAX = 32768,
	NMAX = 256,
	CMAX = 512,
	BASE = 10,
	STRIDE = 10,
	CBASE = 200,
	CSTRIDE = 10,
};

typedef struct Wn Wn;
struct Wn {
	int used;
	int col;			/* which column holds it */
	int dirty;			/* the paper's black box, honestly tracked */
	char name[NMAX];
	long nlen;
	char body[BMAX];
	long blen;
};

typedef struct Col Col;
struct Col {
	int used;
	char tag[NMAX];
	long tlen;
};

static Wn wns[MAXWIN];
static Col cols[MAXCOL];
static int active;			/* the column that last saw action (the paper's rule) */
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

/* ---- the workspace, acme-true ---- */

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

/* the text after the word "Edit" in the tag — the command, acme's way */
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

/* a column: its stack under the column row, and its editable tag —
 * "New Delcol |", the paper's shape at v0 scale */
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
	col->tlen = snprint(col->tag, sizeof col->tag, "New Delcol | ");
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

/* a directory's body is its listing — acme's file browser: names, one
 * per line, directories marked with '/'; look on a name opens it */
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

static int
isdir(char *path)
{
	Dir *d;
	int r;

	d = dirstat(path);
	if(d == nil)
		return -1;			/* does not exist */
	r = (d->mode & DMDIR) != 0;
	free(d);
	return r;
}

/* a window lands in the ACTIVE column — the paper's placement rule,
 * at v0 scale. Tag: name first, commands, | then scratch space. A
 * directory window lists its names and carries no Put. */
static void
mkwin(int k, char *file)
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
	dird = file != nil && isdir(file) == 1;
	if(dird)
		w->nlen = snprint(w->name, sizeof w->name, "%s%s Del Get | ",
			file, file[strlen(file) - 1] == '/' ? "" : "/");
	else
		w->nlen = snprint(w->name, sizeof w->name, "%s Del Get Put Edit | ",
			file == nil ? "" : file);
	if(file != nil){
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

/* the black box: bg flips on the tiny node beside the tag */
static void
setdirty(int k, int on)
{
	Wn *w = &wns[k];
	int base = BASE + STRIDE * k;
	char a[64];

	if(w->dirty == on)
		return;
	w->dirty = on;
	snprint(a, sizeof a, "bg=%s\n", on ? "#000000" : "#eaffff");
	nprint(base + 4, "attrs", a);
	fprint(ctlfd, "sync\n");
}

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

void
main(int argc, char *argv[])
{
	char path[128], line[4096], txt[4096], *f[8], *word, *fn;
	int wid, evfd, fd, nf, i, k, base, off, minted;
	long n, q0, q1, d, fl;
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

	/* the root tag — editable, like every tag — over a ROW of columns */
	fprint(ctlfd, "new 1 edit\nnew 3 stack\n");
	nprint(1, "attrs", "bg=#eaffff\norder=1\n");
	nprint(1, "data", "Newcol Putall Dump Load Exit | ");
	nprint(3, "attrs", "order=2\ndir=row\n");
	mkcol(0);

	k = 0;
	for(; i < argc && k < MAXWIN; i++, k++)
		mkwin(k, argv[i]);
	if(k == 0){
		/* acme's opening face: a second column with the current
		 * directory listed — the file browser */
		static char cwd[256];
		mkcol(1);
		if(getwd(cwd, sizeof cwd) != nil)
			mkwin(k++, cwd);
		else
			mkwin(k++, "/");
	}
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
		word = nil;
		if(nf >= 5 && strcmp(f[0], "execute") == 0){
			unq(f[4], txt, sizeof txt);
			word = txt;
		}
		if(word != nil && i == 1){		/* the root tag's words */
			if(strcmp(word, "Newcol") == 0){
				k = freecol();
				if(k >= 0){
					mkcol(k);
					fprint(ctlfd, "sync\n");
				}
			} else if(strcmp(word, "Putall") == 0){
				for(k = 0; k < MAXWIN; k++){
					if(!wns[k].used)
						continue;
					fn = firstword(wns[k].name, wns[k].nlen, &fl);
					if(fl == 0 || fn[0] == '|')
						continue;
					fd = create(fn, OWRITE, 0644);
					if(fd >= 0){
						write(fd, wns[k].body, wns[k].blen);
						close(fd);
					}
				}
			} else if(strcmp(word, "Dump") == 0){
				fd = create("/tmp/edit.dump", OWRITE, 0644);
				if(fd >= 0){
					for(k = 0; k < MAXWIN; k++){
						if(!wns[k].used)
							continue;
						fn = firstword(wns[k].name, wns[k].nlen, &fl);
						if(fl > 0 && fn[0] != '|')
							fprint(fd, "%s\n", fn);
					}
					close(fd);
				}
			} else if(strcmp(word, "Load") == 0){
				char db[2048], *dl[32];
				int nd, k2;
				n = readfile("/tmp/edit.dump", db, sizeof db);
				if(n > 0){
					nd = getfields(db, dl, 32, 1, "\n");
					for(i = 0; i < nd; i++){
						if(dl[i][0] == 0)
							continue;
						k2 = freewin();
						if(k2 < 0)
							break;
						mkwin(k2, dl[i]);
					}
					fprint(ctlfd, "sync\n");
				}
			} else if(strcmp(word, "Exit") == 0)
				break;
			continue;
		}
		if(i >= CBASE){			/* a column's tag */
			k = (i - CBASE) / CSTRIDE;
			if(k >= MAXCOL || !cols[k].used)
				continue;
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
				continue;
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
				continue;
			}
			if(word == nil)
				continue;
			if(strcmp(word, "New") == 0){
				k = freewin();
				if(k >= 0){
					mkwin(k, nil);
					fprint(ctlfd, "sync\n");
				}
			} else if(strcmp(word, "Delcol") == 0){
				int c2 = (i - CBASE) / CSTRIDE;
				for(k = 0; k < MAXWIN; k++)
					if(wns[k].used && wns[k].col == c2)
						wns[k].used = 0;
				fprint(ctlfd, "del %d\nsync\n", CBASE + CSTRIDE * c2);
				cols[c2].used = 0;
				if(active == c2)
					for(active = 0; active < MAXCOL && !cols[active].used; active++)
						;
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
		active = w->col;
		base = BASE + STRIDE * k;
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
			if(off == 2)
				setdirty(k, 1);
			continue;
		}
		if(nf >= 4 && strcmp(f[0], "delete") == 0 && (off == 1 || off == 2)){
			char *buf = off == 1 ? w->name : w->body;
			long *len = off == 1 ? &w->nlen : &w->blen;
			q0 = atol(f[2]);
			q1 = atol(f[3]);
			if(q1 > *len) q1 = *len;
			if(q0 > q1) q0 = q1;
			memmove(buf + q0, buf + q1, *len - q1);
			*len -= q1 - q0;
			buf[*len] = 0;
			if(off == 2)
				setdirty(k, 1);
			continue;
		}
		if(nf >= 5 && strcmp(f[0], "look") == 0){
			/* acme's B3: a name resolves against the looked-in
			 * window's own directory — the listing is the browser.
			 * In-window literal search is the presenter's half. */
			static char resolved[512];
			char *target;
			unq(f[4], txt, sizeof txt);
			target = txt;
			if(txt[0] != '/' && k < MAXWIN && wns[k].used){
				char dbase[NMAX], *sl;
				long fl2;
				fn = firstword(wns[k].name, wns[k].nlen, &fl2);
				snprint(dbase, sizeof dbase, "%s", fn);
				if(fl2 > 0 && dbase[fl2-1] == '/')
					dbase[fl2-1] = 0;	/* a dir window: itself */
				else if((sl = strrchr(dbase, '/')) != nil)
					*sl = 0;		/* a file window: its dir */
				else
					dbase[0] = 0;
				snprint(resolved, sizeof resolved, "%s/%s", dbase, txt);
				target = resolved;
			}
			if(isdir(target) >= 0){
				int k2 = freewin();
				if(k2 >= 0){
					mkwin(k2, target);
					fprint(ctlfd, "sync\n");
				}
			}
			continue;
		}
		if(word == nil || off != 1)
			continue;
		/* executed in this window's tag: a word, or a SWEPT command
		 * whose first word names the verb and the rest is arguments —
		 * the paper's own execution model */
		{
			char *args = word;
			while(*args != 0 && *args != ' ' && *args != '\t')
				args++;
			if(*args != 0)
				*args++ = 0;
			while(*args == ' ' || *args == '\t')
				args++;
			if(strcmp(word, "Get") == 0){
				fn = firstword(w->name, w->nlen, &fl);
				n = fl > 0 && fn[fl-1] == '/'
					? listdir(fn, w->body, sizeof w->body)
					: readfile(fn, w->body, sizeof w->body);
				if(n >= 0){
					w->blen = n;
					setnode(base + 2, w->body, w->blen);
					setdirty(k, 0);
					fprint(ctlfd, "sync\n");
				}
			} else if(strcmp(word, "Put") == 0){
				fn = firstword(w->name, w->nlen, &fl);
				if(fl == 0)
					continue;
				fd = create(fn, OWRITE, 0644);
				if(fd >= 0){
					write(fd, w->body, w->blen);
					close(fd);
					setdirty(k, 0);
				}
			} else if(strcmp(word, "Del") == 0){
				fprint(ctlfd, "del %d\nsync\n", base);
				w->used = 0;
			} else if(strcmp(word, "Zerox") == 0){
				int k2 = freewin();
				if(k2 >= 0){
					fn = firstword(w->name, w->nlen, &fl);
					mkwin(k2, fl ? fn : nil);
					memmove(wns[k2].body, w->body, w->blen);
					wns[k2].blen = w->blen;
					setnode(BASE + STRIDE * k2 + 2, wns[k2].body, wns[k2].blen);
					fprint(ctlfd, "sync\n");
				}
			} else if(strcmp(word, "Edit") == 0){
				runedit(w, *args != 0 ? args : afteredit(w->name, w->nlen));
				setnode(base + 2, w->body, w->blen);
				setdirty(k, 1);
				fprint(ctlfd, "sync\n");
			}
		}
	}
	snprint(path, sizeof path, "%s/wctl", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "delete"); close(fd); }
	exits(0);
}
