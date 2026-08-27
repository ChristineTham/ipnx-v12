/* libthread for wasm: the real thread.h API as this platform's layer —
 * the vendored library's core is per-architecture scheduler assembly, so
 * this file stands in the same relation to it as u.h does to Plan 9's
 * per-architecture headers. One wasm instance; procs collapse into
 * cooperative threads over the tsave/tjump contexts (guestcore saves the
 * asyncify frames, the stack pointer, AND the shadow-stack region);
 * blocking reads yield to the scheduler, which parks the whole process in
 * the kernel's IOWAIT only when no thread can run.
 *
 * Everything blocking funnels through alt(), as in the real library. */
#include <u.h>
#include <libc.h>
#include <thread.h>

extern int	_tsavec(int);
extern void	_tjumpc(int, int);
extern void	_tdropc(int);
extern long	(*_threadpread)(int, void*, long, vlong);
extern long	_rawpread(int, void*, long, vlong);
extern int	_aread(int, int, int);
extern int	_iowait(void*, int);

enum {
	MAXTHREAD = 64,
	SCHEDCTX = 1,		/* context ids: 1 the scheduler, 2+i the threads */
	IOBUF = 8192,

	Unborn = 0,
	Ready,
	Blocked,		/* on a channel, via alt */
	Ioblocked,		/* on _aread; tag = thread id */
	Dead,
};

enum {
	MAXALT = 16,
	ELEMMAX = 64,		/* channel element size cap */
};

/* A suspended coroutine's STACK is dead storage — its shadow region is
 * saved in its context and the live bytes belong to whoever runs now —
 * so everything a PARTNER touches lives here, off-stack, per thread:
 * the registered ops, a stashed copy of a pending send's value, and the
 * delivery slot a partner fills. The woken thread copies the delivery
 * into its own (freshly restored) frame itself. */
typedef struct AltReg AltReg;
struct AltReg {
	Channel	*c;
	int	op;
	uchar	sval[ELEMMAX];	/* CHANSND: the value, stashed while live */
};

typedef struct Thr Thr;
struct Thr {
	int	state;
	void	(*fn)(void*);
	void	*arg;
	AltReg	areg[MAXALT];	/* while Blocked: registered ops */
	int	nareg;
	int	altfired;	/* which entry a partner completed */
	uchar	altval[ELEMMAX];	/* a partner's delivery (CHANRCV fired) */
	long	ion;		/* while Ioblocked */
	long	ioret;
	uchar	iodata[8192];	/* async read lands here, never on a stack */
	char	name[64];
};

static Thr	thr[MAXTHREAD];
static int	nthreads;
static int	curid = -1;	/* index of the running thread */
static char	*exitsstatus;

__attribute__((weak)) int mainstacksize;

int
threadid(void)
{
	return curid + 1;
}

int
threadcreate(void (*f)(void*), void *arg, uint stack)
{
	int i;

	USED(stack);
	for(i = 0; i < MAXTHREAD; i++)
		if(i >= nthreads || thr[i].state == Dead)
			break;
	if(i == MAXTHREAD)
		sysfatal("threadcreate: out of threads");
	if(i >= nthreads)
		nthreads = i + 1;
	memset(&thr[i], 0, sizeof(Thr));
	thr[i].state = Unborn;
	thr[i].fn = f;
	thr[i].arg = arg;
	return i + 1;
}

int
proccreate(void (*f)(void*), void *arg, uint stack)
{
	/* one wasm instance: a proc IS a thread; its blocking reads yield */
	return threadcreate(f, arg, stack);
}

void
threadexits(char *status)
{
	if(status && status[0])
		exitsstatus = strdup(status);
	thr[curid].state = Dead;
	_tdropc(2 + curid);
	_tjumpc(SCHEDCTX, 1);
}

void
threadexitsall(char *status)
{
	exits(status);
}

void
threadsetname(char *fmt, ...)
{
	va_list arg;

	va_start(arg, fmt);
	vseprint(thr[curid].name, thr[curid].name + sizeof thr[curid].name, fmt, arg);
	va_end(arg);
}

char*
threadgetname(void)
{
	return thr[curid].name;
}

void
yield(void)
{
	/* stay Ready; just give the scheduler a turn */
	if(_tsavec(2 + curid) == 0)
		_tjumpc(SCHEDCTX, 1);
}

static void
sleepyield(void)
{
	/* state already set by the caller; return when readied */
	if(_tsavec(2 + curid) == 0)
		_tjumpc(SCHEDCTX, 1);
}

/* ---- channels: partners are found by scanning the thread table's
 * off-stack registrations; buffers live in the heap ---- */

Channel*
chancreate(int elemsize, int bufsize)
{
	Channel *c;

	if(elemsize > ELEMMAX)
		sysfatal("chancreate: element size %d > %d", elemsize, ELEMMAX);
	c = mallocz(sizeof(Channel) + elemsize * bufsize, 1);
	if(c == nil)
		sysfatal("chancreate: no memory");
	c->e = elemsize;
	c->s = bufsize;
	return c;
}

void
chanfree(Channel *c)
{
	free(c);
}

static uchar*
slot(Channel *c, uint i)
{
	return c->v + (i % c->s) * c->e;
}

static void
copyval(void *dst, void *src, int n)
{
	if(dst != nil && src != nil)
		memmove(dst, src, n);
}

/* a Blocked thread with the complementary op registered on c */
static int
partner(Channel *c, int op, int *entryp)
{
	int t, j;

	for(t = 0; t < nthreads; t++){
		if(thr[t].state != Blocked)
			continue;
		for(j = 0; j < thr[t].nareg; j++)
			if(thr[t].areg[j].c == c
			&& thr[t].areg[j].op == (op == CHANSND ? CHANRCV : CHANSND)){
				*entryp = j;
				return t;
			}
	}
	return -1;
}

static int
opready(Alt *a)
{
	Channel *c = a->c;
	int e;

	if(a->op == CHANSND)
		return (c->s != 0 && c->n < c->s) || partner(c, CHANSND, &e) >= 0;
	if(a->op == CHANRCV)
		return c->n > 0 || partner(c, CHANRCV, &e) >= 0;
	return 0;
}

static void
fire(int t, int entry)
{
	thr[t].altfired = entry;
	thr[t].state = Ready;
}

/* perform a ready op from the RUNNING thread's side; the running thread's
 * own v pointer is live, the partner's data goes through its Thr slots */
static void
run1(Alt *a)
{
	Channel *c = a->c;
	int t, e;

	if(a->op == CHANSND){
		if(c->n == 0 && (t = partner(c, CHANSND, &e)) >= 0){
			copyval(thr[t].altval, a->v, c->e);	/* rendezvous */
			fire(t, e);
			return;
		}
		copyval(slot(c, c->f + c->n), a->v, c->e);	/* buffer */
		c->n++;
		if((t = partner(c, CHANSND, &e)) >= 0){		/* a receiver can go */
			copyval(thr[t].altval, slot(c, c->f), c->e);
			c->f++; c->n--;
			fire(t, e);
		}
	}else{
		if(c->n > 0){
			copyval(a->v, slot(c, c->f), c->e);
			c->f++; c->n--;
			if((t = partner(c, CHANRCV, &e)) >= 0){	/* a sender fits now */
				copyval(slot(c, c->f + c->n), thr[t].areg[e].sval, c->e);
				c->n++;
				fire(t, e);
			}
			return;
		}
		t = partner(c, CHANRCV, &e);
		copyval(a->v, thr[t].areg[e].sval, c->e);	/* rendezvous */
		fire(t, e);
	}
}

int
alt(Alt *alts)
{
	int i, nend, r;
	Thr *t = &thr[curid];

	for(;;){
		nend = -1;
		for(i = 0; ; i++){
			if(alts[i].op == CHANEND || alts[i].op == CHANNOBLK){
				nend = i;
				break;
			}
			if(alts[i].op == CHANNOP)
				continue;
			if(opready(&alts[i])){
				run1(&alts[i]);
				return i;
			}
		}
		if(alts[nend].op == CHANNOBLK)
			return nend;
		if(nend > MAXALT)
			sysfatal("alt: too many channels");
		/* register off-stack (a partner must never chase our frame),
		 * stashing send values while they are still live */
		for(i = 0; i < nend; i++){
			t->areg[i].c = alts[i].op == CHANNOP ? nil : alts[i].c;
			t->areg[i].op = alts[i].op;
			if(alts[i].op == CHANSND)
				copyval(t->areg[i].sval, alts[i].v, alts[i].c->e);
		}
		t->nareg = nend;
		t->altfired = -1;
		t->state = Blocked;
		sleepyield();
		t->nareg = 0;
		if(t->altfired >= 0){
			r = t->altfired;
			if(alts[r].op == CHANRCV)		/* collect the delivery */
				copyval(alts[r].v, t->altval, alts[r].c->e);
			t->altfired = -1;
			return r;
		}
	}
}

static int
chanop1(Channel *c, int op, void *v, int block)
{
	Alt a[2];

	a[0].c = c;
	a[0].v = v;
	a[0].op = op;
	a[1].op = block ? CHANEND : CHANNOBLK;
	if(alt(a) == 0)
		return 1;
	return 0;
}

int send(Channel *c, void *v)	{ return chanop1(c, CHANSND, v, 1); }
int recv(Channel *c, void *v)	{ return chanop1(c, CHANRCV, v, 1); }
int nbsend(Channel *c, void *v)	{ return chanop1(c, CHANSND, v, 0); }
int nbrecv(Channel *c, void *v)	{ return chanop1(c, CHANRCV, v, 0); }

int
sendp(Channel *c, void *v)
{
	return send(c, &v);
}

void*
recvp(Channel *c)
{
	void *v = nil;

	recv(c, &v);
	return v;
}

int
nbsendp(Channel *c, void *v)
{
	return nbsend(c, &v);
}

void*
nbrecvp(Channel *c)
{
	void *v = nil;

	if(nbrecv(c, &v) == 0)
		return nil;
	return v;
}

int
sendul(Channel *c, ulong v)
{
	return send(c, &v);
}

ulong
recvul(Channel *c)
{
	ulong v = 0;

	recv(c, &v);
	return v;
}

int
nbsendul(Channel *c, ulong v)
{
	return nbsend(c, &v);
}

ulong
nbrecvul(Channel *c)
{
	ulong v = 0;

	nbrecv(c, &v);
	return v;
}

/* ---- yielding IO: a read parks the THREAD; the kernel's IOWAIT parks
 * the process only when nothing else can run ---- */

static long
threadpread(int fd, void *buf, long n, vlong off)
{
	Thr *t = &thr[curid];

	USED(off);				/* streams: sequential */
	if(n > sizeof t->iodata)
		n = sizeof t->iodata;
	if(_aread(2 + curid, fd, n) < 0)
		return -1;
	t->ion = n;
	t->ioret = -1;
	t->state = Ioblocked;
	sleepyield();
	if(t->ioret > 0)
		memmove(buf, t->iodata, t->ioret);	/* our frame is live again */
	return t->ioret;
}

static uchar iobuf[IOBUF + 4];

static void
iopump(void)
{
	int n, tag, i;
	Thr *t;

	n = _iowait(iobuf, sizeof iobuf);
	if(n < 4)
		return;
	tag = iobuf[0] | (iobuf[1]<<8) | (iobuf[2]<<16) | (iobuf[3]<<24);
	i = tag - 2;
	if(i < 0 || i >= nthreads || thr[i].state != Ioblocked)
		return;
	t = &thr[i];
	n -= 4;
	if(n > t->ion)
		n = t->ion;
	memmove(t->iodata, iobuf + 4, n);
	t->ioret = n;
	t->state = Ready;
}

/* ---- the scheduler; main() lives here, threadmain() is the program ---- */

static void
schedule(void)
{
	int i, some, io;

	/* Stateless per pass: a thread's return can land in a RESTORED old
	 * scheduler frame (its birth iteration travelled in its context), so
	 * every decision re-reads the global tables and loops back here. */
	for(;;){
		some = 0;
		io = 0;
		for(i = 0; i < nthreads; i++){
			if(thr[i].state == Ioblocked)
				io = 1;
			else if(thr[i].state == Ready || thr[i].state == Unborn)
				some = 1;
		}
		if(!some){
			int blocked = 0;
			if(io){
				iopump();
				continue;
			}
			for(i = 0; i < nthreads; i++)
				if(thr[i].state == Blocked)
					blocked = 1;
			if(blocked)
				sysfatal("all threads blocked (deadlock)");
			break;			/* every thread finished */
		}
		for(i = 0; i < nthreads; i++)
			if(thr[i].state == Ready || thr[i].state == Unborn)
				break;
		if(_tsavec(SCHEDCTX) != 0)
			continue;		/* a thread yielded back; rescan */
		curid = i;
		if(thr[i].state == Unborn){
			thr[i].state = Ready;
			(*thr[i].fn)(thr[i].arg);
			/* returning is exiting, per thread(2) */
			thr[i].state = Dead;
			_tdropc(2 + i);
			continue;
		}
		_tjumpc(2 + i, 1);		/* never returns here */
	}
}

static void
tmain(void *v)
{
	char **argv = v;
	int argc;

	for(argc = 0; argv[argc]; argc++)
		;
	threadmain(argc, argv);
}

void
main(int argc, char *argv[])
{
	USED(argc);
	_threadpread = threadpread;
	threadcreate(tmain, argv, mainstacksize ? mainstacksize : 8192);
	schedule();
	exits(exitsstatus);
}
