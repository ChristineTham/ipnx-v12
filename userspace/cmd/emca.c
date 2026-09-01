/* emca: the IPNX half of the user interface (implementation.md M14c).
 *
 * A FILE SERVER WITH A WORKSPACE, NOT AN EDITOR. Editing is the surface's
 * (design.md 2026-08-31), so none of the interactive machinery lives here —
 * no caret, no selection, no line editor. What emca owns is what IPNX owns:
 *
 *   - the window set, and each window's ONE TAG STRING. The split into
 *     title / toolbar / tag bar is PRESENTATIONAL; the model is one string,
 *     `<name> <fixed builtins> <dynamic> | <scratch>`, with the dynamic block
 *     tracked by offset so the user's own text is never disturbed. This is
 *     acme.c:383's rebuildauto, kept.
 *   - the CORE VERBS. Core verbs are emca's; extra verbs are the type's
 *     (emca.txt) — so the toolbar a window shows is emca's set merged with
 *     whatever /type/<t>/window declares.
 *   - DIRTY STATE, which is what makes Put appear. The paper's rule: Put is
 *     in the tag only while the window is dirty, and its appearance IS the
 *     dirty indicator.
 *   - BUFFERS. N windows may view one buffer, so Zerox aliases rather than
 *     copies, and the recorded divergence retires.
 *   - PLACEMENT. emca watches /dev/window/events and writes each window's
 *     ctl. It is a WATCHER, NOT A GATEKEEPER: any program mints, and with
 *     emca not running a window still opens in its type's default pane.
 *
 * Shape: con(1)'s libthread discipline. A reader thread per event source
 * feeds one consumer through a channel, so a single thread owns every window
 * write and all tag arithmetic — no locks, and shadow state only.
 */
#include <u.h>
#include <libc.h>
#include <thread.h>

enum {
	STACK = 8192,
	MAXWIN = 32,
	NMAX = 512,		/* a tag string */
	BMAX = 65536,		/* a buffer, allocated when a file is opened */
	LMAX = 1024,		/* an event line */
	MAXBUF = 16,
	MROOT = 0, MWIN = 1, MGONE = 2,
};

typedef struct Buf Buf;
struct Buf {
	int used;
	int refs;		/* N windows may view one buffer */
	char path[NMAX];
	char *text;		/* HEAP, not static: --initial-memory is a link-time
				 * constant and a static array is charged before the
				 * module runs. malloc grows the memory instead. */
	long len;
	int dirty;
};

typedef struct Win Win;
struct Win {
	int used;
	int wid;
	char type[64];
	char tag[NMAX];		/* THE ONE STRING */
	long taglen;
	long autopos, autolen;	/* the dynamic block: Put/Undo, tracked by offset */
	char dir[NMAX];		/* the context every command here resolves against */
	Buf *buf;
	int evfd;
};

typedef struct Msg Msg;
struct Msg {
	int kind;
	int wid;
	char line[LMAX];
};

static Channel *mc;
static Win wins[MAXWIN];
static Buf bufs[MAXBUF];
static int rootfd;

/* the core verbs — emca's, on every window whatever its type */
static char *core[] = { "Del", "Snarf", "Get", "Look", "Edit", nil };

static void dolook(Win*, char*);	/* the toolbar's Look and the bar's are one verb */
static void doput(Win*);
static void dodel(Win*);

/* is a window of this type already on this path? Load must not double up */
static Win*
winbypath(char *type, char *path)
{
	int i;

	for(i = 0; i < MAXWIN; i++)
		if(wins[i].used && strcmp(wins[i].type, type) == 0
		&& wins[i].buf != nil && strcmp(wins[i].buf->path, path) == 0)
			return &wins[i];
	return nil;
}

static Win*
winof(int wid)
{
	int i;
	for(i = 0; i < MAXWIN; i++)
		if(wins[i].used && wins[i].wid == wid)
			return &wins[i];
	return nil;
}

static Buf*
bufget(char *path)
{
	int i, fd;
	Buf *b;
	long n;

	for(i = 0; i < MAXBUF; i++)		/* one buffer, many windows */
		if(bufs[i].used && strcmp(bufs[i].path, path) == 0){
			bufs[i].refs++;
			return &bufs[i];
		}
	for(i = 0; i < MAXBUF; i++)
		if(!bufs[i].used)
			break;
	if(i == MAXBUF)
		return nil;
	b = &bufs[i];
	memset(b, 0, sizeof *b);
	b->text = malloc(BMAX);
	if(b->text == nil)
		return nil;
	b->text[0] = 0;
	b->used = 1;
	b->refs = 1;
	strncpy(b->path, path, sizeof b->path - 1);
	fd = open(path, OREAD);
	if(fd >= 0){
		n = read(fd, b->text, BMAX - 1);
		if(n > 0) b->len = n;
		close(fd);
	}
	return b;
}

static void
bufput(Buf *b)
{
	if(b == nil) return;
	if(--b->refs <= 0){
		free(b->text);
		b->text = nil;
		b->used = 0;
	}
}

static void
wfile(Win *w, char *name, char *data, long n)
{
	char p[NMAX];
	int fd;

	snprint(p, sizeof p, "/dev/window/%s/%d/%s", w->type, w->wid, name);
	fd = open(p, OWRITE|OTRUNC);
	if(fd < 0) return;
	write(fd, data, n);
	close(fd);
}

static long
rfile(char *path, char *out, long max)
{
	int fd;
	long n;

	fd = open(path, OREAD);
	if(fd < 0) return -1;
	n = read(fd, out, max - 1);
	close(fd);
	if(n < 0) n = 0;
	out[n] = 0;
	return n;
}

/* the dynamic block: Put appears only while dirty, Undo only when it has
 * work. acme.c:383's rule, and the reason it is tracked by OFFSET is that the
 * user co-authors this string — their text must not shift under them.
 */
static void
rebuildauto(Win *w)
{
	char want[64];
	long wl;

	want[0] = 0;
	if(w->buf != nil && w->buf->dirty)
		strcat(want, " Put");
	wl = strlen(want);
	if(w->autopos > w->taglen) w->autopos = w->taglen;
	if(w->autopos + w->autolen > w->taglen) w->autolen = w->taglen - w->autopos;
	if(wl == w->autolen && memcmp(w->tag + w->autopos, want, wl) == 0)
		return;
	if(w->taglen - w->autolen + wl > NMAX - 1)
		return;
	memmove(w->tag + w->autopos + wl, w->tag + w->autopos + w->autolen,
		w->taglen - w->autopos - w->autolen);
	memmove(w->tag + w->autopos, want, wl);
	w->taglen += wl - w->autolen;
	w->tag[w->taglen] = 0;
	w->autolen = wl;
}

/* the toolbar the surface renders: CORE VERBS ARE EMCA'S, extra verbs are the
 * type's. One control per line, and each names the side that performs it.
 */
static void
pushtoolbar(Win *w)
{
	char out[NMAX * 2], tw[NMAX], p[NMAX];
	long o = 0;
	int i;

	for(i = 0; core[i] != nil; i++)
		o += snprint(out + o, sizeof out - o, "%s ipnx:%s\n", core[i], core[i]);
	if(w->buf != nil && w->buf->dirty)
		o += snprint(out + o, sizeof out - o, "Put ipnx:Put\n");
	o += snprint(out + o, sizeof out - o, "Zerox ipnx:Zerox\n");
	snprint(p, sizeof p, "/type/%s/window", w->type);
	if(rfile(p, tw, sizeof tw) > 0){
		char *s = tw, *e, *sp;
		int i, iscore;
		while(*s){
			e = strchr(s, '\n');
			if(e != nil) *e = 0;
			/* A TYPE MAY NOT REDECLARE A CORE VERB: emca owns those, and
			 * the comparison is by LABEL, not by whole line — otherwise a
			 * type could reintroduce Put with a different action and quietly
			 * defeat the dynamic block, which is the dirty indicator.
			 */
			sp = strchr(s, ' ');
			if(sp != nil) *sp = 0;
			iscore = strcmp(s, "Put") == 0 || strcmp(s, "Zerox") == 0;
			for(i = 0; core[i] != nil && !iscore; i++)
				if(strcmp(s, core[i]) == 0) iscore = 1;
			if(sp != nil) *sp = ' ';
			if(*s && !iscore)
				o += snprint(out + o, sizeof out - o, "%s\n", s);
			if(e == nil) break;
			s = e + 1;
		}
	}
	wfile(w, "toolbar", out, o);
}

/* the tag bar is the SCRATCH region only — everything after the bar. The
 * title is the name and the toolbar is the builtins; one string, three views.
 */
static void
pushtag(Win *w)
{
	char *bar;

	rebuildauto(w);
	bar = strchr(w->tag, '|');
	if(bar == nil) wfile(w, "tag", "", 0);
	else {
		bar++;
		while(*bar == ' ') bar++;
		wfile(w, "tag", bar, strlen(bar));
	}
	pushtoolbar(w);
}

/* one message, allocated by the reader and freed by the consumer. libthread
 * caps a channel element at ELEMMAX (64 bytes), so a line does not travel by
 * value — and passing the pointer is the ownership rule that keeps the single
 * consumer lock-free.
 */
static Msg*
mkmsg(int kind, int wid, char *line)
{
	Msg *m;

	m = malloc(sizeof *m);
	if(m == nil) return nil;
	memset(m, 0, sizeof *m);
	m->kind = kind;
	m->wid = wid;
	if(line != nil)
		strncpy(m->line, line, sizeof m->line - 1);
	return m;
}

static void
post(int kind, int wid, char *line)
{
	Msg *m;

	m = mkmsg(kind, wid, line);
	if(m != nil)
		send(mc, &m);
}

static void
evreader(void *v)
{
	Win *w = v;
	char buf[LMAX];
	long n;

	for(;;){
		n = read(w->evfd, buf, sizeof buf - 1);
		if(n <= 0) break;
		buf[n] = 0;
		post(MWIN, w->wid, buf);
	}
	post(MGONE, w->wid, nil);
	threadexits(nil);
}

static void
rootreader(void *v)
{
	char buf[LMAX];
	long n;

	USED(v);
	for(;;){
		n = read(rootfd, buf, sizeof buf - 1);
		if(n <= 0) break;
		buf[n] = 0;
		post(MROOT, 0, buf);
	}
	threadexits(nil);
}

/* adopt a window someone else minted. emca has no privilege — it learns of
 * the window from the DEVICE, exactly as the surface does.
 */
static void
adopt(char *type, int wid)
{
	Win *w;
	char p[NMAX];
	int i;

	for(i = 0; i < MAXWIN; i++)
		if(!wins[i].used) break;
	if(i == MAXWIN) return;
	w = &wins[i];
	memset(w, 0, sizeof *w);
	w->used = 1;
	w->wid = wid;
	w->evfd = -1;
	strncpy(w->type, type, sizeof w->type - 1);

	/* A window is minted BEFORE its content is known, so nothing is read
	 * here. The content arrives as its own event and setcontent() does the
	 * tag arithmetic then — which is also how the surface reopens a window
	 * on a different file.
	 */
	w->taglen = snprint(w->tag, sizeof w->tag, " Del Snarf Get Look Edit");
	w->autopos = w->taglen;
	w->autolen = 0;
	w->taglen += snprint(w->tag + w->taglen, sizeof w->tag - w->taglen, " | ");

	snprint(p, sizeof p, "/dev/window/%s/%d/events", type, wid);
	w->evfd = open(p, OREAD);
	if(w->evfd >= 0)
		threadcreate(evreader, w, STACK);

	pushtag(w);
}

/* the content arrived, or changed. The NAME is the head of the tag string, so
 * this rewrites exactly that span and leaves the user's own scratch text —
 * which lives after the bar — untouched.
 */
static void
setcontent(Win *w, char *path)
{
	char *slash, *rest;
	char keep[NMAX];
	long hl, kl;

	/* the CONTEXT: every command in this window resolves against it */
	strncpy(w->dir, path, sizeof w->dir - 1);
	w->dir[sizeof w->dir - 1] = 0;
	slash = strrchr(w->dir, '/');
	if(slash != nil) *(slash + 1) = 0;
	else w->dir[0] = 0;

	bufput(w->buf);
	w->buf = nil;
	if(path[0] != 0 && path[strlen(path)-1] != '/')
		w->buf = bufget(path);

	/* everything from the builtins onward survives verbatim */
	rest = strstr(w->tag, " Del Snarf Get Look Edit");
	if(rest == nil) return;
	kl = w->taglen - (rest - w->tag);
	if(kl < 0 || kl > (long)sizeof keep - 1) return;
	memmove(keep, rest, kl);
	keep[kl] = 0;
	hl = strlen(path);
	if(hl + kl > NMAX - 1) return;
	memmove(w->tag, path, hl);
	memmove(w->tag + hl, keep, kl + 1);
	w->autopos += hl - (rest - w->tag);
	w->taglen = hl + kl;
	pushtag(w);
}

static void
drop(Win *w)
{
	if(w->evfd >= 0) close(w->evfd);
	bufput(w->buf);
	w->used = 0;
}

static void
doput(Win *w)
{
	int fd;

	if(w->buf == nil) return;
	fd = create(w->buf->path, OWRITE, 0644);
	if(fd < 0) return;
	write(fd, w->buf->text, w->buf->len);
	close(fd);
	w->buf->dirty = 0;
	pushtag(w);
}

static void
doget(Win *w)
{
	int fd;
	long n;

	if(w->buf == nil) return;
	fd = open(w->buf->path, OREAD);
	if(fd < 0) return;
	n = read(fd, w->buf->text, BMAX - 1);
	close(fd);
	w->buf->len = n > 0 ? n : 0;
	w->buf->dirty = 0;
	pushtag(w);
}

/* Zerox ALIASES: a second window on the same buffer, which is what retires
 * acme's recorded copy-not-alias divergence (design.md 2026-08-31).
 */
static void
dozerox(Win *w)
{
	char p[NMAX], num[32];
	int fd;
	long n;

	if(w->buf == nil) return;
	snprint(p, sizeof p, "/dev/window/%s/clone", w->type);
	fd = open(p, OREAD);
	if(fd < 0) return;
	n = read(fd, num, sizeof num - 1);
	close(fd);
	if(n <= 0) return;
	num[n] = 0;
	snprint(p, sizeof p, "/dev/window/%s/%d/content", w->type, atoi(num));
	fd = open(p, OWRITE|OTRUNC);
	if(fd < 0) return;
	write(fd, w->buf->path, strlen(w->buf->path));
	close(fd);
	/* the root events file announces it; adopt() will share the buffer */
}

static void
dodel(Win *w)
{
	char p[NMAX];
	int fd;

	snprint(p, sizeof p, "/dev/window/%s/%d/wctl", w->type, w->wid);
	fd = open(p, OWRITE);
	if(fd >= 0){ write(fd, "delete\n", 7); close(fd); }
}

/* run a command in the window's directory — the context, which is acme's own
 * invention and the thing native furniture has no concept of.
 */
static void
run(Win *w, char *cmd)
{
	int pid;

	if(cmd == nil || *cmd == 0) return;
	pid = rfork(RFPROC|RFFDG|RFNOWAIT|RFNAMEG|RFENVG);
	if(pid == 0){
		if(w->dir[0]) chdir(w->dir);
		execl("/bin/rc", "rc", "-c", cmd, nil);
		exits("exec");
	}
}

static void
verb(Win *w, char *label)
{
	if(strcmp(label, "Put") == 0) doput(w);
	else if(strcmp(label, "Get") == 0) doget(w);
	else if(strcmp(label, "Del") == 0) dodel(w);
	else if(strcmp(label, "Zerox") == 0) dozerox(w);
	else if(strcmp(label, "Snarf") == 0){ /* the host clipboard IS snarf */ }
	else if(strcmp(label, "Search") == 0){ /* the surface's find — never here */ }
	else if(strcmp(label, "Look") == 0){ /* whole-window Look: its own name */
		if(w->buf != nil) dolook(w, w->buf->path);
	}
	else if(strcmp(label, "Edit") == 0){ /* sam's language, when it lands */ }
	else run(w, label);		/* anything else is a command */
}

/* the surface quotes space, newline and percent so a change is one line.
 * Decoded in place: the result is never longer than the source.
 */
static long
unquote(char *s)
{
	char *r, *out;
	int hi, lo;

	out = s;
	for(r = s; *r; r++){
		if(*r == '%' && r[1] && r[2]){
			hi = r[1] >= 'a' ? r[1]-'a'+10 : r[1] >= 'A' ? r[1]-'A'+10 : r[1]-'0';
			lo = r[2] >= 'a' ? r[2]-'a'+10 : r[2] >= 'A' ? r[2]-'A'+10 : r[2]-'0';
			if(hi >= 0 && hi < 16 && lo >= 0 && lo < 16){
				*out++ = (char)(hi*16 + lo);
				r += 2;
				continue;
			}
		}
		*out++ = *r;
	}
	*out = 0;
	return out - s;
}

/* FNV-1a over the BYTES — the same bytes the offsets are measured in, which
 * is the only way the check survives a multi-byte character.
 */
static ulong
fnv(char *s, long n)
{
	ulong h = 0x811c9dc5UL;
	long i;

	for(i = 0; i < n; i++){
		h ^= (uchar)s[i];
		h *= 0x01000193UL;
		h &= 0xffffffffUL;
	}
	return h;
}

/* THE BUFFER IS EMCA'S. The surface edits its mirror and tells emca what it
 * did; emca applies it here, and Put writes THIS text. Offsets are byte
 * offsets into this buffer.
 */
static void
bufinsert(Buf *b, long off, char *text, long n)
{
	if(b == nil || n <= 0) return;
	if(off < 0) off = 0;
	if(off > b->len) off = b->len;
	if(b->len + n > BMAX - 1) return;
	memmove(b->text + off + n, b->text + off, b->len - off);
	memmove(b->text + off, text, n);
	b->len += n;
	b->text[b->len] = 0;
}

static void
bufdelete(Buf *b, long from, long to)
{
	if(b == nil) return;
	if(from < 0) from = 0;
	if(to > b->len) to = b->len;
	if(to <= from) return;
	memmove(b->text + from, b->text + to, b->len - to);
	b->len -= to - from;
	b->text[b->len] = 0;
}

/* resolve a range against the window's directory — the context, which is
 * acme's own invention and the thing native furniture has no concept of
 */
static void
resolve(Win *w, char *s, char *out, int max)
{
	if(s[0] == '/' || w->dir[0] == 0) snprint(out, max, "%s", s);
	else snprint(out, max, "%s%s", w->dir, s);
	cleanname(out);
}

/* Is this range an ADDRESS? sam's forms, unchanged — what changes is that
 * emca now REPORTS the judgement instead of silently acting on it, which is
 * the whole of "the bar SHOWS the choice" (emca.txt).
 */
static int
isaddr(char *s)
{
	char *p;
	long n;

	if(*s == 0) return 0;
	if(*s == ':') s++;
	if(*s == 0) return 0;
	if(*s == '/'){
		/* A REGEXP ADDRESS IS DELIMITED AT BOTH ENDS. Without that, every
		 * absolute path reads as one — /etc/motd offered Jump, which is
		 * precisely the silent misjudgement the bar exists to expose.
		 */
		n = strlen(s);
		return n >= 3 && s[n-1] == '/';
	}
	if(*s == '#' || *s == '$' || *s == '.') return 1;
	for(p = s; *p; p++)
		if(*p < '0' || *p > '9') return 0;
	return 1;				/* a bare line number */
}

static int
ispath(Win *w, char *s, char *out, int max)
{
	Dir *d;

	if(*s == 0 || strlen(s) > 200) return 0;
	if(strchr(s, ' ') != nil || strchr(s, '\n') != nil) return 0;
	resolve(w, s, out, max);
	d = dirstat(out);
	if(d == nil) return 0;
	free(d);
	return 1;
}

/* LOOK is the plumber's judgement. Until the plumber lands, emca does what
 * acme's look does in the overwhelming case: a path opens in a window of the
 * type the file's own shape implies.
 */
static void
dolook(Win *w, char *sel)
{
	char full[NMAX], p[NMAX], num[32];
	char *type;
	Dir *d;
	int fd;
	long n;

	if(!ispath(w, sel, full, sizeof full)) return;
	d = dirstat(full);
	if(d == nil) return;
	type = (d->qid.type & QTDIR) ? "dir" : "text";
	free(d);
	snprint(p, sizeof p, "/dev/window/%s/clone", type);
	fd = open(p, OREAD);
	if(fd < 0) return;
	n = read(fd, num, sizeof num - 1);
	close(fd);
	if(n <= 0) return;
	num[n] = 0;
	snprint(p, sizeof p, "/dev/window/%s/%d/content", type, atoi(num));
	fd = open(p, OWRITE|OTRUNC);
	if(fd < 0) return;
	write(fd, full, strlen(full));
	close(fd);
}

static void
onwinline(Win *w, char *line)
{
	char *sp;

	while(*line == ' ') line++;
	sp = strchr(line, ' ');

	if(strncmp(line, "exec ", 5) == 0){
		verb(w, line + 5);
		return;
	}
	if(strncmp(line, "tag ", 4) == 0){	/* the user's own text, executed */
		run(w, line + 4);
		return;
	}
	if(strncmp(line, "snarf ", 6) == 0){	/* the surface copied — /dev/snarf is the sync point */
		char *s = line + 6;
		int fd;
		unquote(s);
		fd = open("/dev/snarf", OWRITE|OTRUNC);
		if(fd >= 0){ write(fd, s, strlen(s)); close(fd); }
		return;
	}
	if(strncmp(line, "execute ", 8) == 0){	/* execute <q0> <q1> <text> */
		char cmd[NMAX * 2];
		char *q, *r;
		q = strchr(line + 8, ' ');
		r = q != nil ? strchr(q + 1, ' ') : nil;
		if(r == nil) return;
		r++;
		unquote(r);
		USED(cmd);
		run(w, r);
		return;
	}
	/* `look` was ONE verb; the bar shows three, and Open is the one that is
	 * IPNX's — it mints a window and resolves against the namespace. Jump and
	 * Search never arrive here: they are the surface's, by the same rule that
	 * separates ipnx: from host: on the toolbar.
	 */
	if(strncmp(line, "look ", 5) == 0 || strncmp(line, "open ", 5) == 0){
		char *q, *r;
		q = strchr(line + 5, ' ');
		r = q != nil ? strchr(q + 1, ' ') : nil;
		if(r != nil){ r++; unquote(r); dolook(w, r); }
		return;
	}
	if(strcmp(line, "put") == 0){
		/* THE SURFACE PUT, and it wrote the file itself — it holds the real
		 * editor's byte-exact text, where this buffer is reconstructed from
		 * a change stream. So this is a NOTIFICATION, not a command: emca
		 * re-reads what actually landed rather than writing over it. Two
		 * writers to one file is the bug; one writer and a re-read is not.
		 */
		doget(w);
		return;
	}
	if(strncmp(line, "dirty ", 6) == 0){
		if(w->buf != nil){
			w->buf->dirty = atoi(line + 6);
			pushtag(w);
		}
		return;
	}
	if(strncmp(line, "insert ", 7) == 0){	/* insert <off> <quoted text> */
		char *q;
		long off, n;
		if(w->buf == nil) return;
		q = strchr(line + 7, ' ');
		off = atoi(line + 7);
		if(q != nil){
			q++;
			n = unquote(q);
			bufinsert(w->buf, off, q, n);
		}
		if(!w->buf->dirty){ w->buf->dirty = 1; pushtag(w); }
		return;
	}
	if(strncmp(line, "delete ", 7) == 0){	/* delete <from> <to> */
		char *q;
		if(w->buf == nil) return;
		q = strchr(line + 7, ' ');
		if(q != nil)
			bufdelete(w->buf, atoi(line + 7), atoi(q + 1));
		if(!w->buf->dirty){ w->buf->dirty = 1; pushtag(w); }
		return;
	}
	if(strncmp(line, "seq ", 4) == 0){	/* seq <n> <hash> — the mirror check */
		char *q;
		if(w->buf == nil) return;
		q = strchr(line + 4, ' ');
		if(q == nil) return;
		/* Silent divergence is the failure worth spending bytes on: say so
		 * loudly rather than write a file that is neither copy.
		 */
		if(strtoul(q + 1, nil, 16) != fnv(w->buf->text, w->buf->len))
			fprint(2, "emca: window %d diverged from its mirror at seq %d\n",
				w->wid, atoi(line + 4));
		return;
	}
	USED(sp);
}

/* THE WORKSPACE VERBS (emca.txt's toolbar table: Putall Dump Load Exit). Their
 * operand is the workspace, so they arrive on the workspace's own events file —
 * the exact parallel of a window verb arriving on that window's.
 */
static char*
dumppath(void)
{
	static char p[NMAX];
	char *h;

	h = getenv("home");
	if(h == nil || *h == 0) h = "/tmp";
	snprint(p, sizeof p, "%s/emca.dump", h);
	return p;
}

static void
doputall(void)
{
	int i;

	for(i = 0; i < MAXWIN; i++)
		if(wins[i].used && wins[i].buf != nil && wins[i].buf->dirty)
			doput(&wins[i]);
}

/* The dump is a FILE, and deliberately one a person can read and edit — the
 * house style. One line per window: <type> <path>, in the order they were
 * adopted, which is the order Load re-opens them.
 */
static void
dodump(void)
{
	char line[NMAX];
	int i, fd;

	fd = create(dumppath(), OWRITE, 0644);
	if(fd < 0){
		fprint(2, "emca: Dump: %r\n");
		return;
	}
	for(i = 0; i < MAXWIN; i++){
		if(!wins[i].used) continue;
		snprint(line, sizeof line, "%s %s\n", wins[i].type,
			wins[i].buf != nil ? wins[i].buf->path : wins[i].dir);
		write(fd, line, strlen(line));
	}
	close(fd);
}

/* mint a window of <type> on <path> — what emcaopen does, from inside emca */
static void
openwin(char *type, char *path)
{
	char p[NMAX], num[32];
	int fd;
	long n;

	snprint(p, sizeof p, "/dev/window/%s/clone", type);
	fd = open(p, OREAD);
	if(fd < 0) return;
	n = read(fd, num, sizeof num - 1);
	close(fd);
	if(n <= 0) return;
	num[n] = 0;
	snprint(p, sizeof p, "/dev/window/%s/%d/content", type, atoi(num));
	fd = open(p, OWRITE|OTRUNC);
	if(fd < 0) return;
	write(fd, path, strlen(path));
	close(fd);
}

static void
doload(void)
{
	char buf[BMAX], *s, *e, *sp;

	if(rfile(dumppath(), buf, sizeof buf) <= 0){
		fprint(2, "emca: Load: no %s\n", dumppath());
		return;
	}
	for(s = buf; *s; s = e + 1){
		e = strchr(s, '\n');
		if(e != nil) *e = 0;
		sp = strchr(s, ' ');
		if(sp != nil && sp[1] != 0){
			*sp = 0;
			if(winbypath(s, sp + 1) == nil)	/* already open: leave it */
				openwin(s, sp + 1);
		}
		if(e == nil) break;
	}
}

/* acme's Exit, kept: it REFUSES while anything is unwritten. The refusal is the
 * feature — losing work to a menu item is the failure this verb exists to avoid.
 */
static void
doexit(void)
{
	int i, dirty = 0;

	for(i = 0; i < MAXWIN; i++)
		if(wins[i].used && wins[i].buf != nil && wins[i].buf->dirty)
			dirty++;
	if(dirty > 0){
		fprint(2, "emca: Exit: %d window%s unwritten — Put or Putall first\n",
			dirty, dirty == 1 ? "" : "s");
		return;
	}
	for(i = 0; i < MAXWIN; i++)
		if(wins[i].used)
			dodel(&wins[i]);
	threadexitsall(nil);
}

static void
onrootline(char *line)
{
	char type[64];
	int wid;
	Win *w;
	char *p;

	while(*line == ' ') line++;
	/* the workspace's own verbs — operand is the workspace, so they arrive
	 * here rather than on any window (emca.txt: operand determines surface)
	 */
	if(strcmp(line, "Putall") == 0){ doputall(); return; }
	if(strcmp(line, "Dump") == 0){ dodump(); return; }
	if(strcmp(line, "Load") == 0){ doload(); return; }
	if(strcmp(line, "Exit") == 0){ doexit(); return; }
	if(strncmp(line, "content ", 8) == 0){		/* content <type> <n> <path> */
		char *q, *r;
		Win *cw;
		p = line + 8;
		while(*p == ' ') p++;
		q = strchr(p, ' ');			/* end of type */
		if(q == nil) return;
		r = strchr(q + 1, ' ');			/* end of number */
		cw = winof(atoi(q + 1));
		if(cw == nil) return;
		if(r == nil) setcontent(cw, "");
		else {
			while(*r == ' ') r++;
			setcontent(cw, r);
		}
		return;
	}
	if(strncmp(line, "new ", 4) == 0){		/* new <type> <n> */
		char *q;
		long tl;
		p = line + 4;
		while(*p == ' ') p++;
		q = strchr(p, ' ');
		if(q == nil) return;
		tl = q - p;
		if(tl <= 0 || tl >= (long)sizeof type) return;
		memmove(type, p, tl);
		type[tl] = 0;
		wid = atoi(q + 1);
		if(wid > 0 && winof(wid) == nil)
			adopt(type, wid);
		return;
	}
	if(strncmp(line, "del ", 4) == 0){
		wid = atoi(line + 4);
		w = winof(wid);
		if(w != nil) drop(w);
		return;
	}
}

void
threadmain(int argc, char **argv)
{
	Msg *m;
	Win *w;
	char *s, *e;

	USED(argc); USED(argv);

	rootfd = open("/dev/window/events", OREAD);
	if(rootfd < 0)
		sysfatal("emca: no /dev/window — this kernel has no control interface");

	mc = chancreate(sizeof(Msg*), 8);
	threadcreate(rootreader, nil, STACK);

	for(;;){
		if(recv(mc, &m) < 0) break;
		s = m->line;
		while(*s){			/* a read may carry several lines */
			e = strchr(s, '\n');
			if(e != nil) *e = 0;
			if(*s){
				if(m->kind == MROOT) onrootline(s);
				else if(m->kind == MWIN){
					w = winof(m->wid);
					if(w != nil) onwinline(w, s);
				}
			}
			if(e == nil) break;
			s = e + 1;
		}
		if(m->kind == MGONE){
			w = winof(m->wid);
			if(w != nil) drop(w);
		}
		free(m);
	}
	threadexitsall(nil);
}
