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
 *     (emca.md) — so the toolbar a window shows is emca's set merged with
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
#include "../libc/lib9p.h"	/* M17a1: emca serves its windows over wire 9P */

enum {
	STACK = 8192,
	MAXWIN = 32,
	NMAX = 512,		/* a tag string */
	BMAX = 65536,		/* a buffer, allocated when a file is opened */
	LMAX = 1024,		/* an event line */
	MAXBUF = 16,
	MAXKIDS = 32,		/* M17a2: emca holds the tree, so the bound lives here */
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
	long osize;		/* a USER RESIZE: an override along the parent's axis,
				 * remembered until Fit drops it. 0 means none. */
	long iw, ih;		/* a picture's intrinsic size, from its header —
				 * or from the surface's `size` event, for the
				 * formats emca cannot parse */
	Buf *buf;
	int evfd;

	/* M17a2 — THE TREE, held here rather than read from the kernel on
	 * every question. emca is the window manager, so the relationships
	 * between windows are its state; the kernel is being removed from
	 * this entirely (docs/window.md). These mirror the device today and
	 * become authoritative when it goes.
	 */
	int parent;		/* the window that allocates this one; -1 at the root */
	int kid[MAXKIDS];	/* children, IN ORDER — order is the layout */
	int nkid;
	int axis;		/* 1 row, 2 col, 0 for a window holding a body.
				 * Alternation is the invariant: always
				 * perpendicular to the parent's. */
	int allocated;		/* given a rectangle; otherwise it is a TAB */
	int premax[MAXKIDS];	/* which of MY kids were allocated before a
				 * maximise — the memory that makes it a
				 * toggle rather than a one-way door */
	int npremax;
	int maxed;

	/* M17a1 — what the CONTRACT says a window reports (docs/window.md).
	 * emca serves these; today it also still tells the kernel, and the
	 * suite compares the two. When the device goes, only this remains.
	 */
	long rx, ry, rw, rh;	/* the CONTENT rectangle — chrome subtracted */
	long minw, minh;	/* what the manager said it needs ... */
	long natw, nath;	/* ... and what it would like */
	char role[16];		/* look, edit, manage, shell, properties */
	char title[NMAX];	/* emca OWNS this; the manager may only look */
	char status[128];	/* the status line — consequential, not visible elsewhere */
	char verbs[NMAX];	/* the toolbar, as pushtoolbar computed it */
	int dirty;
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

/* NOTHING IS UNIVERSAL ON THE TOOLBAR (emca.md, "The window toolbar, by
 * type"). Not Save — a shell has nothing to save. Not Revert — a shell does
 * not. Not even Undo, which belongs to `edit` alone, because Undo is available
 * exactly where every operation a window offers stays in a buffer emca holds,
 * and a killed process does not come back.
 *
 * The window OPERATIONS (Close, Minimise, Maximise, New column/row/tab, Fit)
 * are not here either: their operand is the window as a thing in a layout, so
 * they sit with the controls. What is left for this file is the TYPE's list,
 * plus Save while the buffer is dirty — acme's rule with acme's name
 * translated (acme.c:383).
 */
static char *core[] = { nil };

static void dolook(Win*, char*);	/* the toolbar's Look and the bar's are one verb */
static void doreset(int);		/* the root type's verb, reached from its toolbar */
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

	o = 0;
	if(w->buf != nil && w->buf->dirty)
		o += snprint(out + o, sizeof out - o, "Save ipnx:Save\n");
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

	/* M17a1 — the same list IS the contract's `verbs` (docs/window.md).
	 * Stashing what was just computed keeps one merge rule: a second
	 * implementation here is a second answer waiting to disagree.
	 */
	if(o > (long)sizeof w->verbs - 1) o = sizeof w->verbs - 1;
	memmove(w->verbs, out, o);
	w->verbs[o] = 0;
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
static void wctlf(int, char*, ...);
static void treelink(int, int, int, int);
static void treeunlink(int);
static int nkids(int, int*);
static void dodel(Win*);
static void setcontent(Win*, char*);
static void srvpost(void);
static void srvunpost(void);

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
	w->parent = -1;		/* until the tree says otherwise */
	w->nkid = 0;
	w->axis = 0;		/* a body, not a container, until it divides */
	w->allocated = 1;
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

	/* emca owns the title and the manager may only look at it — so it is
	 * set HERE, where emca learns what the window holds, and nowhere else */
	strncpy(w->title, path, sizeof w->title - 1);
	w->title[sizeof w->title - 1] = 0;

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
	int i;

	/* a window that has gone leaves the tree FIRST — otherwise emca goes on
	 * allocating space to a window that is not there, and a parent's kid
	 * list outlives its kids. Its own children are orphaned rather than
	 * destroyed: the kernel's win_close is recursive and will announce each
	 * one in turn, so each unlinks itself as its notice arrives. */
	for(i = 0; i < w->nkid; i++){
		Win *c = winof(w->kid[i]);
		if(c != nil) c->parent = -1;
	}
	w->nkid = 0;
	treeunlink(w->wid);

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

/* CLOSING A CONTAINER CLOSES WHAT IT HOLDS — acme's own colcloseall(): a
 * column IS a window, so this is one rule and not two. The kernel did this
 * recursion while it held the tree; it holds no tree now, so the recursion
 * came here with the state. Depth first, because a parent's `delete` must not
 * strand the children it was holding.
 */
static void
dodel(Win *w)
{
	char p[NMAX];
	int kids[MAXKIDS], nk, i, fd;

	nk = nkids(w->wid, kids);
	for(i = 0; i < nk; i++){
		Win *c = winof(kids[i]);
		if(c != nil) dodel(c);
	}
	treeunlink(w->wid);

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
	/* emca uses the era's names; acme's port keeps Snarf, Put and Get,
	 * because renaming acme's buttons would be changing acme (emca.md) */
	if(strcmp(label, "Save") == 0) doput(w);
	else if(strcmp(label, "Revert") == 0) doget(w);
	else if(strcmp(label, "Reset") == 0) doreset(w->wid);
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
 * the whole of "the bar SHOWS the choice" (emca.md).
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

/* ---- THE SIZING HEURISTIC (emca.md, "Sizing: automatic, content-aware") ----
 *
 * acme's coladd() shrinks the victim to min(half its height, THE SPACE ITS
 * CONTENT OCCUPIES) — content-aware, not fractional, which is where
 * "automatic and reasonable" comes from. emca generalises it: every window
 * declares a MINIMUM and a NATURAL along its parent's axis; minimums are met
 * first, the remainder is shared in proportion to the SLACK (natural minus
 * minimum) and capped at natural, and anything still left is shared equally.
 * acme's rule then falls out rather than being ported — a sibling with little
 * content has a lot of slack, a full one has none.
 *
 * The unit is device-independent pixels with a text cell reported alongside
 * (emca.md); until the surface reports one (M15d) these are the defaults.
 */
enum {
	FURN = 3,			/* title, tag line, status: three lines */
	MINCOLS = 20, MINLINES = 1,
	NATCOLS = 80, NATLINES = 10,
	LEAFCOLS = 72,			/* the classic measure: what a leaf needs */
};

/* THE UNIT IS DEVICE-INDEPENDENT PIXELS, and the host reports THE TEXT CELL in
 * the same unit (emca.md, "The unit, and why it is not characters"). The
 * character rule survives as arithmetic — a leaf is LEAFCOLS * cellw — so
 * raising the reader's text size enlarges the reported cell and the
 * breakpoints move for free, which is WCAG 1.4.4 holding by construction.
 * Characters cannot be the UNIT, because not every window is text: an image
 * has an aspect ratio and no columns at all.
 *
 * These are the defaults until a surface says otherwise.
 */
static long cellw = 8, cellh = 18;

static long
wread(int wid, char *file, char *out, long max)
{
	char p[NMAX];

	snprint(p, sizeof p, "/dev/window/%d/%s", wid, file);
	return rfile(p, out, max);
}

/* ── M17a2: THE TREE IS EMCA'S ────────────────────────────────────────────
 *
 * These four answered by reading the kernel's window device until 2026-09-03.
 * They now answer from emca's own `Win` records, which is what "the window
 * manager owns the relationships between windows" means in code: a window's
 * IDENTITY is still minted by `#w/<type>/clone` (that is the raster side, and
 * it moves to the host in M17a3), but a window's PLACE among other windows is
 * decided here and stored here.
 *
 * THE KERNEL HAS NO TREE AT ALL as of 2026-09-03 — no parent, no kids, no
 * axis, no allocation, and none of the verbs that changed them. Nothing here
 * mirrors anywhere; this is the only copy. emca serves it at /dev/emca/<n>/
 * for tools, and a manager's own /dev/window/ shows none of it, because a
 * manager has no business seeing the arrangement it sits in.
 */
static int
naxis(int wid)
{
	Win *w = winof(wid);
	return w != nil ? w->axis : 0;
}

static int
nallocated(int wid)
{
	Win *w = winof(wid);
	return w != nil ? w->allocated : 1;
}

/* kids are held IN ORDER, because the order IS the layout */
static int
nkids(int wid, int *out)
{
	Win *w = winof(wid);
	int i;

	if(w == nil) return 0;
	for(i = 0; i < w->nkid; i++)
		out[i] = w->kid[i];
	return w->nkid;
}

/* ── and the three operations that change it ──────────────────────────────
 * Each updates emca FIRST and then tells the kernel, never the reverse: emca
 * is the one deciding, so a mirror that failed must not be able to change
 * what emca believes.
 */
static void
treeaxis(int wid, int ax)
{
	Win *w = winof(wid);

	if(w != nil) w->axis = ax;
}

static void
treeunlink(int wid)
{
	Win *w, *pw;
	int i, j;

	if((w = winof(wid)) == nil) return;
	if((pw = winof(w->parent)) != nil){
		for(i = 0; i < pw->nkid; i++)
			if(pw->kid[i] == wid){
				for(j = i; j + 1 < pw->nkid; j++)
					pw->kid[j] = pw->kid[j + 1];
				pw->nkid--;
				break;
			}
	}
	w->parent = -1;
}

/* MINT A CHILD. The two halves of "new window" come from different places
 * now, and that split IS M17a2: `#w/<type>/clone` supplies the IDENTITY —
 * it is the raster surface, and it moves to the host in M17a3 — while the
 * PLACE the window takes in the tree is decided here.
 *
 * clone returns the id synchronously, so emca adopts it on the spot rather
 * than waiting for the `new` announcement to come back round the message
 * loop; the announcement then finds the window already known and skips.
 */
static void adopt(char*, int);

static int
treemint(char *type, char *content)
{
	char p[NMAX], b[32];
	int kid, fd;

	snprint(p, sizeof p, "/dev/window/%s/clone", type);
	if(rfile(p, b, sizeof b) <= 0) return -1;
	if((kid = atoi(b)) <= 0) return -1;
	if(winof(kid) == nil) adopt(type, kid);
	if(content != nil && *content){
		/* a split's children INHERIT the content, which is what makes
		 * "New column" a duplicate rather than an empty pane */
		snprint(p, sizeof p, "/dev/window/%s/%d/content", type, kid);
		if((fd = open(p, OWRITE|OTRUNC)) >= 0){
			write(fd, content, strlen(content));
			close(fd);
		}
	}
	return kid;
}

static int
treenewkid(int parent, char *type, int tab)
{
	int kid;

	if((kid = treemint(type, nil)) < 0) return -1;
	treelink(parent, kid, -1, !tab);
	return kid;
}

/* `at` < 0 appends. Order is the layout, so a position is not decoration. */
static void
treelink(int parent, int wid, int at, int allocated)
{
	Win *w, *pw;
	int i;

	if((w = winof(wid)) == nil || (pw = winof(parent)) == nil) return;

	/* TWO REFUSALS, and a tree without them is not a tree. A window may
	 * not be its own parent, and it may not become a child of its own
	 * descendant — either would make a cycle, and the layout walk would
	 * then never terminate. The kernel refused both; so must emca, or the
	 * guarantee moved out with the state and did not arrive.
	 */
	if(parent == wid) return;
	for(i = pw->parent; i > 0; ){
		Win *up = winof(i);
		if(i == wid) return;
		i = up != nil ? up->parent : -1;
	}

	treeunlink(wid);
	if(pw->nkid >= MAXKIDS) return;
	if(at < 0 || at > pw->nkid) at = pw->nkid;
	for(i = pw->nkid; i > at; i--)
		pw->kid[i] = pw->kid[i - 1];
	pw->kid[at] = wid;
	pw->nkid++;
	w->parent = parent;
	w->allocated = allocated;
}


/* INTRINSIC SIZE FOR A PICTURE. emca does not render an image, but it must
 * know how much room to ask for, so it reads the DIMENSIONS OUT OF THE HEADER
 * — bounded work, a header and never a decode. This is what "it knows what an
 * image is (or video, or postscript etc) and what aspect ratio is" means in
 * practice, and it is why the unit cannot be characters: a picture has an
 * aspect ratio and no columns at all.
 *
 * Anything not recognised returns 0 and the window is sized as text; a surface
 * that has actually decoded the thing corrects emca with a `size <w> <h>`
 * event, so layout never blocks on a decode and emca is never wrong for long.
 */
static int
imagesize(char *path, long *iw, long *ih)
{
	uchar b[1024];
	int fd;
	long n, i;

	fd = open(path, OREAD);
	if(fd < 0) return 0;
	n = read(fd, b, sizeof b);
	close(fd);
	if(n < 16) return 0;

	/* PNG: the IHDR chunk is always first, at a fixed offset */
	if(b[0]==0x89 && b[1]=='P' && b[2]=='N' && b[3]=='G'){
		*iw = (b[16]<<24)|(b[17]<<16)|(b[18]<<8)|b[19];
		*ih = (b[20]<<24)|(b[21]<<16)|(b[22]<<8)|b[23];
		return *iw > 0 && *ih > 0;
	}
	/* GIF: little-endian, in the logical screen descriptor */
	if(b[0]=='G' && b[1]=='I' && b[2]=='F'){
		*iw = b[6] | (b[7]<<8);
		*ih = b[8] | (b[9]<<8);
		return *iw > 0 && *ih > 0;
	}
	/* JPEG: walk the segments to the first start-of-frame */
	if(b[0]==0xFF && b[1]==0xD8){
		for(i = 2; i + 9 < n; ){
			if(b[i] != 0xFF){ i++; continue; }
			while(i < n && b[i] == 0xFF) i++;
			if(i >= n) break;
			{
				int m = b[i];
				long seg = (b[i+1]<<8)|b[i+2];
				if((m >= 0xC0 && m <= 0xC3) || (m >= 0xC5 && m <= 0xC7)
				|| (m >= 0xC9 && m <= 0xCB) || (m >= 0xCD && m <= 0xCF)){
					*ih = (b[i+4]<<8)|b[i+5];
					*iw = (b[i+6]<<8)|b[i+7];
					return *iw > 0 && *ih > 0;
				}
				if(seg <= 0) break;
				i += 1 + seg;
			}
		}
		return 0;
	}
	/* SVG: text, so viewBox or width/height in the root element */
	b[n < (long)sizeof b ? n : (long)sizeof b - 1] = 0;
	if(strstr((char*)b, "<svg") != nil){
		char *v = strstr((char*)b, "viewBox");
		if(v != nil){
			char *f[8];
			char tmp[128];
			v = strchr(v, '"');
			if(v != nil){
				strncpy(tmp, v + 1, sizeof tmp - 1);
				tmp[sizeof tmp - 1] = 0;
				if(strchr(tmp, '"') != nil) *strchr(tmp, '"') = 0;
				if(tokenize(tmp, f, 8) == 4){
					*iw = atoi(f[2]);
					*ih = atoi(f[3]);
					return *iw > 0 && *ih > 0;
				}
			}
		}
	}
	return 0;
}

/* the content this window shows, as a path */
static int
contentpath(int wid, char *out, long max)
{
	return wread(wid, "content", out, max) > 0 && out[0] != 0;
}

/* how many lines this window's content wants — emca has the buffer, which is
 * exactly why the geometry is emca's and not the surface's */
static long
contentlines(int wid)
{
	Win *w;
	long i, lines;

	w = winof(wid);
	if(w == nil || w->buf == nil) return NATLINES;
	lines = 1;
	for(i = 0; i < w->buf->len; i++)
		if(w->buf->text[i] == '\n') lines++;
	if(lines < 1) lines = 1;
	return lines;
}

static long minalong(int wid, int axis);
static long natalong(int wid, int axis);

/* along a ROW the measure is width; along a COL it is height */
static long
minalong(int wid, int axis)
{
	int kids[MAXKIDS], nk, i, ax;
	long total, m;

	ax = naxis(wid);
	nk = ax ? nkids(wid, kids) : 0;
	if(nk == 0)
		return axis == 1 ? MINCOLS * cellw : (FURN + MINLINES) * cellh;
	total = 0;
	for(i = 0; i < nk; i++){
		if(!nallocated(kids[i])) continue;
		m = minalong(kids[i], axis);
		if(ax == axis) total += m;		/* stacked along this axis */
		else if(m > total) total = m;		/* side by side across it */
	}
	if(total == 0) total = axis == 1 ? MINCOLS * cellw : (FURN + MINLINES) * cellh;
	return total;
}

static long
natalong(int wid, int axis)
{
	int kids[MAXKIDS], nk, i, ax;
	long total, v;

	ax = naxis(wid);
	nk = ax ? nkids(wid, kids) : 0;
	if(nk == 0){
		char path[NMAX];
		long piw, pih;
		Win *pw;

		/* A PICTURE'S NATURAL SIZE IS ITS INTRINSIC SIZE, in the same
		 * device-independent unit. Deliberately not "the width it was
		 * given, divided by the aspect" — that would make the natural
		 * depend on the allocation, and Fit would stop being idempotent.
		 * The surface scales to fit at render time; emca only has to ask
		 * for room in the right proportion.
		 */
		pw = winof(wid);
		if(pw != nil && pw->iw > 0 && pw->ih > 0)
			return axis == 1 ? pw->iw : pw->ih + FURN * cellh;
		if(contentpath(wid, path, sizeof path) && imagesize(path, &piw, &pih)){
			if(pw != nil){ pw->iw = piw; pw->ih = pih; }
			return axis == 1 ? piw : pih + FURN * cellh;
		}
		return axis == 1 ? NATCOLS * cellw
				 : (contentlines(wid) + FURN) * cellh;
	}
	total = 0;
	for(i = 0; i < nk; i++){
		if(!nallocated(kids[i])) continue;
		v = natalong(kids[i], axis);
		if(ax == axis) total += v;
		else if(v > total) total = v;
	}
	return total;
}

static void
setrect(int wid, long x, long y, long w, long h)
{
	char p[NMAX], line[128];
	Win *ww;
	int fd;

	/* M17a1 — emca DECIDED this rectangle, so emca keeps it. Telling the
	 * kernel is what still happens; being asked for it is what emca now
	 * answers itself, and that asymmetry is the whole move.
	 */
	if((ww = winof(wid)) != nil){
		ww->rx = x; ww->ry = y; ww->rw = w; ww->rh = h;
	}

	snprint(p, sizeof p, "/dev/window/%d/wctl", wid);
	fd = open(p, OWRITE);
	if(fd < 0) return;
	snprint(line, sizeof line, "rect %ld %ld %ld %ld\n", x, y, w, h);
	write(fd, line, strlen(line));
	close(fd);
}

/* divide this window's rectangle among the children it has allocated to, then
 * let each do the same. Depth first, because a child's share is decided before
 * it divides it.
 */
static void
allocate(int wid, long x, long y, long w, long h)
{
	int kids[MAXKIDS], nk, i, ax, na;
	int al[MAXKIDS];
	long mins[MAXKIDS], nats[MAXKIDS], give[MAXKIDS], ovr[MAXKIDS];
	long along, summin, slack, want, left, extra, at;
	Win *cw;
	int nshare;

	setrect(wid, x, y, w, h);
	ax = naxis(wid);
	if(ax == 0) return;
	nk = nkids(wid, kids);
	na = 0;
	for(i = 0; i < nk; i++)
		if(nallocated(kids[i])) al[na++] = kids[i];
	if(na == 0) return;

	along = ax == 1 ? w : h;
	summin = 0;
	want = 0;
	nshare = 0;
	/* A USER RESIZE IS AN OVERRIDE, and it is honoured until Fit drops it —
	 * which is what keeps "resizing is optional and by taste" true: the
	 * automatic rule goes on running for every window nobody has touched.
	 */
	for(i = 0; i < na; i++){
		cw = winof(al[i]);
		ovr[i] = cw != nil ? cw->osize : 0;
		mins[i] = minalong(al[i], ax);
		nats[i] = natalong(al[i], ax);
		if(nats[i] < mins[i]) nats[i] = mins[i];
		if(ovr[i] > 0){
			if(ovr[i] < mins[i]) ovr[i] = mins[i];
			along -= ovr[i];
			continue;
		}
		nshare++;
		summin += mins[i];
		want += nats[i] - mins[i];
	}
	if(along < 0) along = 0;
	/* 1. everyone gets their minimum; 2. the remainder is shared by SLACK,
	 * capped at natural; 3. anything still over is shared equally, given to
	 * nobody in particular — privileging one window is the kind of help
	 * that reads as the layout fighting you.
	 */
	slack = along - summin;
	if(slack < 0) slack = 0;
	left = slack;
	for(i = 0; i < na; i++){
		if(ovr[i] > 0){ give[i] = ovr[i]; continue; }
		extra = want > 0 ? slack * (nats[i] - mins[i]) / want : 0;
		if(mins[i] + extra > nats[i]) extra = nats[i] - mins[i];
		give[i] = mins[i] + extra;
		left -= extra;
	}
	if(left > 0 && nshare > 0){
		int j = 0;
		for(i = 0; i < na; i++){
			if(ovr[i] > 0) continue;
			give[i] += left / nshare + (j < left % nshare ? 1 : 0);
			j++;
		}
	}

	at = ax == 1 ? x : y;
	for(i = 0; i < na; i++){
		if(ax == 1) allocate(al[i], at, y, give[i], h);
		else        allocate(al[i], x, at, w, give[i]);
		at += give[i];
	}
}

/* lay this subtree out from its own rectangle, honouring overrides */
static void
relayout(int wid)
{
	char b[128];
	char *f[6];

	if(wread(wid, "wctl", b, sizeof b) <= 0) return;
	if(tokenize(b, f, 6) < 4) return;
	allocate(wid, atoi(f[0]), atoi(f[1]), atoi(f[2]), atoi(f[3]));
}

/* the window that ALLOCATES this one — a resize must re-lay-out the siblings,
 * not just the window that was dragged */
static int
parentof(int wid)
{
	Win *w = winof(wid);
	return w != nil && w->parent > 0 ? w->parent : 0;
}

static void
clearoverrides(int wid)
{
	int kids[MAXKIDS], nk, i;
	Win *w;

	w = winof(wid);
	if(w != nil) w->osize = 0;
	nk = nkids(wid, kids);
	for(i = 0; i < nk; i++)
		clearoverrides(kids[i]);
}

static void
wctlf(int wid, char *fmt, ...)
{
	char p[NMAX], line[256];
	va_list a;
	int fd;

	snprint(p, sizeof p, "/dev/window/%d/wctl", wid);
	fd = open(p, OWRITE);
	if(fd < 0) return;
	va_start(a, fmt);
	vsnprint(line, sizeof line - 2, fmt, a);
	va_end(a);
	strcat(line, "\n");
	write(fd, line, strlen(line));
	close(fd);
}

/* THE ROOT WINDOW'S CONVENTION (emca.md PART FIVE). The root is the screen —
 * the window with no parent — and it divides ITSELF. What it divides into is a
 * CONVENTION it follows, not a structure the system knows about: nothing
 * downstream can tell these columns from any others, because there is no pane
 * type and no reserved name.
 *
 *   leaves = cols / 72, the classic measure, computed from the REPORTED cell
 *
 *   1 leaf    small    one column — the root itself, holding windows stacked
 *   2         medium   two columns
 *   3         large    three columns
 *   4+        xlarge   four columns, each dividing further
 */
/* move everything `from` holds into `to` — used when a column is about to go,
 * so its windows survive it */
static void
hoist(int from, int to)
{
	int kids[MAXKIDS], nk, i;

	nk = nkids(from, kids);
	for(i = 0; i < nk; i++)
		treelink(to, kids[i], -1, nallocated(kids[i]));
}

static void
convention(int wid)
{
	char b[128];
	char *f[6];
	long w, cols;
	int leaves, want, i, nk;
	int kids[MAXKIDS];

	if(wread(wid, "wctl", b, sizeof b) <= 0) return;
	if(tokenize(b, f, 6) < 4) return;
	w = atoi(f[2]);
	if(w <= 0 || cellw <= 0) return;
	cols = w / cellw;
	leaves = cols / LEAFCOLS;
	want = leaves >= 4 ? 4 : leaves >= 3 ? 3 : leaves >= 2 ? 2 : 1;

	if(want == 1){
		/* one column IS the root: windows stack in it directly, and no
		 * degenerate single-child container is created. Anything already
		 * in a column moves up rather than going with it — NOTHING
		 * DISAPPEARS, in either direction.
		 */
		nk = nkids(wid, kids);
		for(i = 0; i < nk; i++)
			hoist(kids[i], wid);
		nk = nkids(wid, kids);
		for(i = 0; i < nk; i++){
			treeunlink(kids[i]);
			wctlf(kids[i], "delete");
		}
		treeaxis(wid, 2);
	} else {
		if(naxis(wid) != 1) treeaxis(wid, 1);
		nk = nkids(wid, kids);
		/* CROSSING A BREAKPOINT RESTRUCTURES, IT DOES NOT DESTROY. Grow
		 * by adding columns; shrink by emptying the surplus into the last
		 * survivor first, so no window is lost to a window being resized.
		 */
		for(i = nk; i < want; i++){
			Win *pw = winof(wid);
			treenewkid(wid, pw != nil ? pw->type : "root", 0);
		}
		if(nk > want){
			for(i = want; i < nk; i++)
				hoist(kids[i], kids[want - 1]);
			for(i = want; i < nk; i++){
				treeunlink(kids[i]);
				wctlf(kids[i], "delete");
			}
		}
	}
	relayout(wid);
}

/* SPLIT: this window gains a sibling under a container on `ax`. acme's
 * coladd is the shape — the window being split keeps its content and the new
 * one starts empty — and a TAB is the same operation with the allocation bit
 * cleared, which is why `newtab` is not a third verb's worth of code.
 */
static void
dosplit(Win *w, int ax, int allocated)
{
	char *content;
	Win *pw;
	int p, sib, first, second, at, i;

	content = w->buf != nil ? w->buf->path : nil;
	p = parentof(w->wid);
	pw = winof(p);

	/* A SIBLING, where there is a parent already dividing on this axis —
	 * and a tab is ALWAYS a sibling wherever there is a parent to be one
	 * in, because a tab has no axis of its own. */
	if((pw != nil && pw->axis == ax) || (!allocated && pw != nil)){
		if((sib = treemint(w->type, content)) < 0) return;
		at = -1;
		for(i = 0; i < pw->nkid; i++)
			if(pw->kid[i] == w->wid){ at = i + 1; break; }
		treelink(p, sib, at, allocated);
		relayout(p);
		return;
	}

	/* OTHERWISE W BECOMES A CONTAINER: its content moves into a first
	 * child and the duplicate becomes the second, so the window the person
	 * was looking at is still there — it is just now one of two. */
	if((first = treemint(w->type, content)) < 0) return;
	if((second = treemint(w->type, content)) < 0) return;
	treeaxis(w->wid, ax);
	treelink(w->wid, first, -1, 1);
	treelink(w->wid, second, -1, allocated);
	setcontent(w, "");		/* a container holds windows, not content */
	relayout(w->wid);
}

/* MAXIMISE: everyone else leaves the allocation, and pressing it again puts
 * them back. It toggles on the SIBLINGS, not on the window — which is what
 * makes it the same operation as minimise with the argument inverted.
 */
static void
domaximise(Win *w)
{
	Win *pw, *s;
	int i, j, p, was;

	if((p = parentof(w->wid)) == 0) return;	/* nothing to maximise within */
	if((pw = winof(p)) == nil) return;

	if(pw->maxed){
		/* PUT THE ARRANGEMENT BACK exactly as it was — which is why the
		 * parent remembers a LIST and not a flag. Restoring by rule
		 * rather than by memory would silently allocate windows the
		 * person had minimised themselves. */
		for(i = 0; i < pw->nkid; i++){
			if((s = winof(pw->kid[i])) == nil) continue;
			was = 0;
			for(j = 0; j < pw->npremax; j++)
				if(pw->premax[j] == pw->kid[i]) was = 1;
			s->allocated = was;
		}
		pw->maxed = 0;
		pw->npremax = 0;
	} else {
		pw->npremax = 0;
		for(i = 0; i < pw->nkid; i++){
			if((s = winof(pw->kid[i])) == nil) continue;
			if(s->allocated && pw->npremax < MAXKIDS)
				pw->premax[pw->npremax++] = pw->kid[i];
			s->allocated = pw->kid[i] == w->wid;
		}
		pw->maxed = 1;
	}
	relayout(p);
}

/* Reset: rebuild the root from its convention. DESTRUCTIVE of structure, which
 * is what separates it from Fit — and it is the root type's verb rather than a
 * core one, because only the root has a default to return to.
 */
static void
doreset(int wid)
{
	int kids[MAXKIDS], nk, i;

	/* Reset IS destructive — that is the difference from Fit. It discards
	 * the arrangement entirely and rebuilds from the convention, which is
	 * what brings back columns someone closed.
	 */
	nk = nkids(wid, kids);
	for(i = 0; i < nk; i++){
		treeunlink(kids[i]);
		wctlf(kids[i], "delete");
	}
	treeaxis(wid, 0);
	convention(wid);
}

/* Fit: re-derive this subtree's allocation from the rule, discarding whatever
 * sizes were dragged into place. Non-destructive of STRUCTURE, which is what
 * separates it from Reset — and idempotent, which is what makes it testable.
 */
static void
dofit(int wid)
{
	char b[128];
	long x, y, w, h;
	char *f[6];
	int nf;

	clearoverrides(wid);
	if(wread(wid, "wctl", b, sizeof b) <= 0) return;
	nf = tokenize(b, f, 6);
	if(nf < 4) return;
	x = atoi(f[0]); y = atoi(f[1]); w = atoi(f[2]); h = atoi(f[3]);
	allocate(wid, x, y, w, h);
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
	if(strcmp(line, "fit") == 0){ dofit(w->wid); return; }
	if(strcmp(line, "reset") == 0){ doreset(w->wid); return; }

	/* ── M17a2: THE TREE VERBS ARRIVE HERE, because the tree is emca's ──
	 *
	 * These used to be written straight to the kernel's `wctl`, which is
	 * the surface talking PAST the window manager — the same coupling the
	 * rectangle had (RESEARCH §9.9). A tree the kernel mutates is a tree
	 * emca does not know, so the door has to be emca's own.
	 *
	 * newrow/newcol/newtab are ONE operation with two parameters — the
	 * axis to split on, and whether the new window is allocated or a tab.
	 * That is the kernel's own reading of them and it survives the move.
	 */
	if(strncmp(line, "reparent ", 9) == 0){	/* reparent <parent> [<at>] */
		char *q = line + 9;
		int np, at = -1;
		while(*q == ' ') q++;
		np = atoi(q);
		if((q = strchr(q, ' ')) != nil) at = atoi(q + 1);
		treelink(np, w->wid, at, w->allocated);
		relayout(np);
		return;
	}
	/* DELETE IS A TREE VERB when the window is a container, because closing
	 * one closes what it holds — and that recursion is emca's since the
	 * kernel stopped holding a tree. Written to the kernel's wctl it would
	 * close this window and strand its children. */
	if(strcmp(line, "delete") == 0){ dodel(w); return; }
	if(strcmp(line, "newcol") == 0){ dosplit(w, 1, 1); return; }
	if(strcmp(line, "newrow") == 0){ dosplit(w, 2, 1); return; }
	if(strcmp(line, "newtab") == 0){ dosplit(w, 1, 0); return; }
	if(strcmp(line, "minimise") == 0 || strcmp(line, "minimize") == 0){
		/* minimise(me) moves ME out of the allocation; maximise(me)
		 * moves everyone else out. One operation, two arguments —
		 * which is why there is no third concept here either. */
		w->allocated = !w->allocated;
		relayout(parentof(w->wid));
		return;
	}
	if(strcmp(line, "maximise") == 0 || strcmp(line, "maximize") == 0){
		domaximise(w);
		return;
	}
	/* the surface decoded something emca could not parse — video,
	 * PostScript, a format it does not know — and says how big it is */
	if(strncmp(line, "size ", 5) == 0){
		char *q;
		q = strchr(line + 5, ' ');
		if(q == nil) return;
		w->iw = atoi(line + 5);
		w->ih = atoi(q + 1);
		relayout(parentof(w->wid));
		return;
	}
	/* resize <w> <h> [<cellw> <cellh>] — ONE VERB, and the extra fields
	 * carry extra information rather than changing what it means. From a
	 * user dragging a divider it is two fields; from a surface reporting
	 * its viewport it is four, because only the surface knows the text
	 * cell — that depends on the rendered font and the reader's text-size
	 * setting, both of which are the surface's half.
	 *
	 * Hers: "Host tells emca - we have a 800x1024 window. emca says 'Ok,
	 * we need to apply this responsive layout' create these windows."
	 */
	if(strncmp(line, "resize ", 7) == 0){
		char *f[6];
		char buf[128];
		int nf, p;
		long nw, nh;

		strncpy(buf, line + 7, sizeof buf - 1);
		buf[sizeof buf - 1] = 0;
		nf = tokenize(buf, f, 6);
		if(nf < 2) return;
		nw = atoi(f[0]);
		nh = atoi(f[1]);
		if(nf >= 4){			/* the surface reported its cell */
			if(atoi(f[2]) > 0) cellw = atoi(f[2]);
			if(atoi(f[3]) > 0) cellh = atoi(f[3]);
		}
		p = parentof(w->wid);
		if(p > 0){
			/* a child's size is its own to choose only along its
			 * parent's axis; the other direction is the parent's */
			w->osize = naxis(p) == 1 ? nw : nh;
			relayout(p);
		} else {
			/* no parent: this IS the viewport. Set it, and if the
			 * root has not divided itself yet, apply the convention
			 * now that there is a geometry to divide.
			 */
			setrect(w->wid, 0, 0, nw, nh);
			/* THE CONVENTION IS THE ROOT TYPE'S, not every
			 * parentless window's. An ordinary window with no
			 * parent is just a window; only `root` is the screen,
			 * and only it divides itself by convention.
			 */
			if(strcmp(w->type, "root") == 0)
				convention(w->wid);
			else
				relayout(w->wid);
		}
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

/* THE WORKSPACE VERBS (emca.md's toolbar table: Putall Dump Load Exit). Their
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
	srvunpost();		/* a posted channel outliving its server is a door onto nothing */
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
	 * here rather than on any window (emca.md: operand determines surface)
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
	srvpost();		/* M17a1: emca serves its windows as files */

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

/* ═══════════════════════════════════════════════════════════════════════
 * M17a1 — EMCA IS A FILE SERVER.
 *
 * docs/window.md: "The manager interface is a file interface. emca serves one
 * directory per window, and a manager reads and writes files in it." Until
 * now emca was a CLIENT of the kernel's window device — it posted messages
 * and read the tree back. That is the wrong way round: the window manager
 * owns windows, so the window manager serves them.
 *
 * The tree is two levels and the qid is arithmetic rather than a table:
 *
 *     /              the set
 *     /<n>/          one window, named by its id
 *     /<n>/{rect,size,verbs,status,dirty,type,role,title,events,ctl}
 *
 *     qpath = 0 for the root, (wid+1)*NQF + file for everything else,
 *     so walk, stat and read all decode a qid by division. No node table
 *     can drift out of step with the window table this way.
 *
 * What this does NOT do yet is take the tree (M17a2) or the raster (M17a3).
 * Both sides answer today and the suite compares them; that is what makes
 * the next two moves verifiable rather than hopeful.
 * ═══════════════════════════════════════════════════════════════════════ */

enum {
	NQF = 32,		/* qid stride: files per window, a power of two */
	Qroot = 0,
	Qwin = 0,		/* the window's own directory */
	Qrect, Qsize, Qverbs, Qstatus, Qdirty,
	Qtype, Qrole, Qtitle, Qevents, Qctl,
	/* THE TREE (design.md, 2026-09-03). Served in the TOOL view only: a
	 * manager's own /dev/window/ gains nothing, so no manager can see the
	 * arrangement it sits in, and the contract is untouched. */
	Qparent, Qaxis, Qalloc, Qwinid,
	Qkids,			/* the kids DIRECTORY */
	Qmax,
	NSFID = 64,
};

static char *qnames[] = {
	nil, "rect", "size", "verbs", "status", "dirty",
	"type", "role", "title", "events", "ctl",
	"parent", "axis", "alloc", "winid", "kids",
};

static int sfid[NSFID], sfq[NSFID];	/* fid -> qid path */
static uchar smsg[MSIZE9], sout[MSIZE9];
static int srvfd = -1;

#define QWID(q)  (((q) / NQF) - 1)	/* which window a qid names */
#define QFILE(q) ((q) % NQF)		/* which of its files */
#define SQTYPE(q) (((q) == Qroot || QFILE(q) == Qwin || QFILE(q) == Qkids) ? QTDIR9 : QTFILE9)

static int
sfind(int fid, int alloc)
{
	int i, free_ = -1;

	for(i = 0; i < NSFID; i++){
		if(sfid[i] == fid) return i;
		if(sfid[i] == -1 && free_ < 0) free_ = i;
	}
	if(alloc && free_ >= 0){
		sfid[free_] = fid;
		return free_;
	}
	return -1;
}

/* the CONTENT of one file, rendered from emca's own state. Every answer here
 * is something emca decided or was told — nothing is fetched from the kernel,
 * which is the point of the exercise.
 */
static int
sread(int q, char *out, int max)
{
	Win *w;
	int f, i, n;

	f = QFILE(q);
	if((w = winof(QWID(q))) == nil) return -1;
	switch(f){
	case Qrect:   return snprint(out, max, "%ld %ld %ld %ld\n", w->rx, w->ry, w->rw, w->rh);
	case Qsize:   return snprint(out, max, "%ld %ld %ld %ld\n", w->minw, w->minh, w->natw, w->nath);
	case Qstatus: return snprint(out, max, "%s\n", w->status);
	case Qdirty:  return snprint(out, max, "%d\n", w->dirty);
	case Qtype:   return snprint(out, max, "%s\n", w->type);
	case Qrole:   return snprint(out, max, "%s\n", w->role[0] ? w->role : "look");
	case Qtitle:  return snprint(out, max, "%s\n", w->title);
	case Qverbs:
		/* one verb per line, the same grammar as /type/<x>/verbs —
		 * because it IS that list, with the core verbs merged in */
		n = strlen(w->verbs);
		if(n > max) n = max;
		memmove(out, w->verbs, n);
		return n;
	case Qwinid:  return snprint(out, max, "%d\n", w->wid);
	case Qparent: return w->parent > 0 ? snprint(out, max, "%d\n", w->parent) : 0;
	/* EMPTY means "holds a body, not children" — the kernel's own
	 * convention, kept verbatim so nothing is invented while moving */
	case Qaxis:   return w->axis == 0 ? 0
			: snprint(out, max, "%s\n", w->axis == 1 ? "row" : "col");
	case Qalloc:  return snprint(out, max, "%s\n", w->allocated ? "allocated" : "tab");
	case Qevents: return 0;		/* a stream: nothing buffered, no EOF */
	case Qctl:    return 0;		/* write-only in effect */
	}
	return -1;
}

static uchar *
spstat(uchar *p, int q, char *asname)
{
	uchar *sz = p;
	char nm[32];
	int f, isdir;

	f = QFILE(q);
	isdir = SQTYPE(q) == QTDIR9;
	if(asname != nil) strncpy(nm, asname, sizeof nm - 1);
	else if(q == Qroot) strcpy(nm, "/");
	else if(f == Qwin) snprint(nm, sizeof nm, "%d", QWID(q));
	else strncpy(nm, qnames[f], sizeof nm - 1);
	nm[sizeof nm - 1] = 0;

	p = put16(p, 0);
	p = put16(p, 0); p = put32(p, 0);
	p = putqid(p, isdir ? QTDIR9 : QTFILE9, q + 1);
	p = put32(p, isdir ? (0x80000000|0755) : 0666);
	p = put32(p, 0); p = put32(p, 0);
	p = put64(p, 0);
	p = putstr(p, nm);
	p = putstr(p, "emca"); p = putstr(p, "emca"); p = putstr(p, "emca");
	put16(sz, p - sz - 2);
	return p;
}

/* walk ONE name from q; -1 if it is not there. Two levels, so this is two
 * cases and no recursion. */
static int
swalk1(int q, char *name)
{
	Win *w;
	int i;

	if(q == Qroot){
		if(strcmp(name, "..") == 0) return Qroot;
		if((w = winof(atoi(name))) == nil) return -1;
		return (w->wid + 1) * NQF + Qwin;
	}
	/* inside kids/: the name is a POSITION and walking it lands on that
	 * child's OWN directory — so the tree is navigable with no second
	 * vocabulary, and `kids/0/parent` is the same file as `<child>/parent` */
	if(QFILE(q) == Qkids){
		if(strcmp(name, "..") == 0) return (QWID(q) + 1) * NQF + Qwin;
		if((w = winof(QWID(q))) == nil) return -1;
		i = atoi(name);
		if(name[0] < '0' || name[0] > '9' || i < 0 || i >= w->nkid) return -1;
		return (w->kid[i] + 1) * NQF + Qwin;
	}
	if(QFILE(q) != Qwin) return -1;
	if(strcmp(name, "..") == 0) return Qroot;
	for(i = Qrect; i < Qmax; i++)
		if(strcmp(name, qnames[i]) == 0)
			return (QWID(q) + 1) * NQF + i;
	return -1;
}

/* a directory read: the set of windows at the root, the ten files inside one.
 * Offsets are honoured the cheap way — regenerate and skip — because these
 * directories are tens of entries, never thousands.
 */
/* A directory read: the window set at the root, the ten files inside one.
 * Offsets are honoured by REGENERATING and skipping — these directories hold
 * tens of entries, never thousands, so the cost is nothing and the code has
 * no cursor to keep in step with a window table that changes underneath it.
 * read(5)'s rule that a record is never split is what the budget check is for.
 */
static uchar *
sreaddir(uchar *p, int q, uvlong off, uint count)
{
	uchar *st, tmp[512];
	uvlong seen = 0;
	uint used = 0;
	int i, n;

	for(i = 0; ; i++){
		if(q == Qroot){
			if(i >= MAXWIN) break;
			if(!wins[i].used) continue;
			st = spstat(tmp, (wins[i].wid + 1) * NQF + Qwin, nil);
		} else if(QFILE(q) == Qkids){
			/* POSITIONS, in order — `ls` sorts, so the listing IS
			 * the arrangement. The qid is the child's own
			 * directory; only the NAME is positional. */
			Win *pw = winof(QWID(q));
			char pos[16];
			if(pw == nil || i >= pw->nkid) break;
			snprint(pos, sizeof pos, "%d", i);
			st = spstat(tmp, (pw->kid[i] + 1) * NQF + Qwin, pos);
		} else {
			if(Qrect + i >= Qmax) break;
			st = spstat(tmp, (QWID(q) + 1) * NQF + Qrect + i, nil);
		}
		n = st - tmp;
		if(seen < off){ seen += n; continue; }	/* before the offset */
		if(used + n > count) break;		/* never split a record */
		memmove(p, tmp, n);
		p += n;
		used += n;
	}
	return p;
}

static void
srvthread(void *v)
{
	int fd, type, tag, fid, nfid, nf, nq, q, i, n9;
	uchar *p, *b;
	uint n, count;
	uvlong off;
	char nm[NMAX], buf[BMAX/8];
	Win *w;

	fd = (int)(uintptr)v;
	for(i = 0; i < NSFID; i++) sfid[i] = -1;

	for(;;){
		n = read9msg(fd, smsg);
		if(n == 0 || (long)n < 0) break;
		type = smsg[4];
		tag = get16(smsg + 5);
		p = sout + 7;
		b = smsg + 7;

		switch(type){
		case Tversion:
			p = put32(p, MSIZE9);
			p = putstr(p, "9P2000");
			send9msg(fd, type + 1, tag, sout, p);
			break;
		case Tattach:
			fid = get32(b);
			if((nf = sfind(fid, 1)) < 0){ send9err(fd, tag, "no fids", sout); break; }
			sfq[nf] = Qroot;
			p = putqid(p, QTDIR9, Qroot + 1);
			send9msg(fd, type + 1, tag, sout, p);
			break;
		case Twalk:
			/* fid[4] newfid[4] nwname[2] nwname*(wname[s]) */
			fid = get32(b); b += 4;
			nfid = get32(b); b += 4;
			nq = get16(b); b += 2;
			if((nf = sfind(fid, 0)) < 0){ send9err(fd, tag, "bad fid", sout); break; }
			q = sfq[nf];
			p = put16(p, 0);			/* nwqid, patched below */
			for(i = 0; i < nq; i++){
				n9 = get16(b); b += 2;
				if(n9 >= (int)sizeof nm) n9 = sizeof nm - 1;
				memmove(nm, b, n9); nm[n9] = 0; b += n9;
				if((q = swalk1(q, nm)) < 0) break;
				p = putqid(p, SQTYPE(q), q + 1);
			}
			put16(sout + 7, i);
			/* a walk that got the whole way BINDS newfid; a partial one
			 * binds nothing, which is walk(5)'s rule and the reason a
			 * failed walk leaves the client's old fid intact */
			if(i == nq && (n9 = sfind(nfid, 1)) >= 0)
				sfq[n9] = q;
			send9msg(fd, type + 1, tag, sout, p);
			break;
		case Topen:
			fid = get32(b);
			if((nf = sfind(fid, 0)) < 0){ send9err(fd, tag, "bad fid", sout); break; }
			q = sfq[nf];
			p = putqid(p, SQTYPE(q), q + 1);
			p = put32(p, MSIZE9 - 24);
			send9msg(fd, type + 1, tag, sout, p);
			break;
		case Tread:
			fid = get32(b); b += 4;
			off = get64(b); b += 8;
			count = get32(b);
			if((nf = sfind(fid, 0)) < 0){ send9err(fd, tag, "bad fid", sout); break; }
			q = sfq[nf];
			if(count > MSIZE9 - 24) count = MSIZE9 - 24;
			if(SQTYPE(q) == QTDIR9){
				b = sreaddir(p + 4, q, off, count);
				put32(p, b - (p + 4));
				send9msg(fd, type + 1, tag, sout, b);
			} else {
				n9 = sread(q, buf, sizeof buf);
				if(n9 < 0){ send9err(fd, tag, "gone", sout); break; }
				if(off >= (uvlong)n9) n9 = 0;
				else { n9 -= off; memmove(buf, buf + off, n9); }
				if((uint)n9 > count) n9 = count;
				p = put32(p, n9);
				memmove(p, buf, n9); p += n9;
				send9msg(fd, type + 1, tag, sout, p);
			}
			break;
		case Twrite:
			fid = get32(b); b += 4;
			off = get64(b); b += 8;
			count = get32(b); b += 4;
			if((nf = sfind(fid, 0)) < 0){ send9err(fd, tag, "bad fid", sout); break; }
			q = sfq[nf];
			if(count >= sizeof buf) count = sizeof buf - 1;
			memmove(buf, b, count); buf[count] = 0;
			if((w = winof(QWID(q))) == nil){ send9err(fd, tag, "gone", sout); break; }
			switch(QFILE(q)){
			case Qsize:
				w->minw = strtol(buf, &b, 10); w->minh = strtol((char*)b, &b, 10);
				w->natw = strtol((char*)b, &b, 10); w->nath = strtol((char*)b, &b, 10);
				break;
			case Qstatus:
				strncpy(w->status, buf, sizeof w->status - 1);
				if((b = (uchar*)strchr(w->status, '\n')) != nil) *b = 0;
				break;
			case Qdirty:
				w->dirty = atoi(buf);
				break;
			case Qctl:
				/* window-level requests: a child INFORMS its parent,
				 * which acts — so these route into emca's own verbs */
				if(strncmp(buf, "close", 5) == 0) dodel(w);
				else if(strncmp(buf, "duplicate", 9) == 0) dozerox(w);
				break;
			default:
				send9err(fd, tag, "read-only", sout);
				goto next;
			}
			p = put32(p, count);
			send9msg(fd, type + 1, tag, sout, p);
			break;
		case Tclunk:
		case Tremove:
			fid = get32(b);
			if((nf = sfind(fid, 0)) >= 0) sfid[nf] = -1;
			send9msg(fd, Tclunk + 1, tag, sout, p);
			break;
		case Tstat:
			fid = get32(b);
			if((nf = sfind(fid, 0)) < 0){ send9err(fd, tag, "bad fid", sout); break; }
			/* Rstat is nstat[2] then the record: put16 ADVANCES, so the
			 * record goes at p+2 and nstat is patched at p itself */
			b = spstat(p + 2, sfq[nf], nil);
			put16(p, b - (p + 2));
			send9msg(fd, type + 1, tag, sout, b);
			break;
		default:
			send9err(fd, tag, "emca: unsupported", sout);
			break;
		}
	next:;
	}
	close(fd);
}

/* THE /srv NAME IS THE EXTERNAL DOOR, NOT THE MANAGER'S DOOR.
 *
 * A manager reaches its window at `/dev/window/` in its OWN namespace, which
 * emca mounts for it — no name, no id, nothing global (docs/window.md). That
 * is rio's shape and it has no collision to have. This post exists for
 * everything OUTSIDE emca's process tree: a window tool, a debugger, the
 * suite.
 *
 * And it MUST be instance-unique, because `#s` is one table for the whole
 * kernel (kernel/src/lib.rs, `srv_posts`) while every other name a process
 * sees is namespace-local. emca NESTS by design, so a fixed name is a
 * collision by construction rather than a hypothetical — the second emca
 * would silently fail to post and serve a door nobody could find.
 *
 * `<user>.<pid>` is the Plan 9 answer and it is already in this tree: the real
 * acme posts `/srv/acme.%s.%d` (acme.c:321). Same convention, same reason.
 */
static char srvname[64];

static void
srvunpost(void)
{
	if(srvname[0] != 0){
		remove(srvname);
		srvname[0] = 0;
	}
}

static void
srvpost(void)
{
	int fd[2], sfd;
	char b[32];
	char *u;

	if(pipe(fd) < 0) return;
	u = getuser();
	snprint(srvname, sizeof srvname, "/srv/emca.%s.%d",
		u != nil && *u ? u : "none", getpid());
	if((sfd = create(srvname, OWRITE, 0600)) < 0){
		/* no /srv bound at all: serve anyway. The no-srv case is not a
		 * special case — a nested emca whose parent gave it no /srv
		 * still has managers, and they never needed this door.
		 */
		srvname[0] = 0;
		close(fd[1]);
		srvfd = fd[0];
		threadcreate(srvthread, (void*)(uintptr)fd[0], STACK);
		return;
	}
	snprint(b, sizeof b, "%d", fd[1]);
	write(sfd, b, strlen(b));
	close(sfd);
	close(fd[1]);
	srvfd = fd[0];
	threadcreate(srvthread, (void*)(uintptr)fd[0], STACK);
}
