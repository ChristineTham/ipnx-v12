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
extern int	(*_threadrfork)(int);
extern int	(*_threadawait)(char*, int);
extern int	(*_threadsleep)(long);
extern long	_rawpread(int, void*, long, vlong);
extern int	_aread(int, int, int);
extern int	_iowait(void*, int, int);
extern int	_rawawait(char*, int, int);

enum {
	MAXTHREAD = 64,
	SCHEDCTX = 1,		/* context ids: 1 the scheduler, 2+i the threads */
	IOBUF = 8192,

	Unborn = 0,
	Ready,
	Blocked,		/* on a channel, via alt */
	Ioblocked,		/* on _aread; tag = thread id */
	Sleeping,		/* until wakeat (thread-aware sleep) */
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
	vlong	wakeat;		/* while Sleeping: nsec() deadline */
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

static int
threadsleep(long ms)
{
	Thr *t = &thr[curid];

	if(ms <= 0){
		yield();
		return 0;
	}
	t->wakeat = nsec() + (vlong)ms * 1000000LL;
	t->state = Sleeping;
	sleepyield();
	return 0;
}

/* wake sleepers whose time has come; return ms to the nearest deadline */
static int
wakesleepers(void)
{
	vlong now, nearest;
	int i, ms;

	nearest = 0;
	now = nsec();
	for(i = 0; i < nthreads; i++){
		if(thr[i].state != Sleeping)
			continue;
		if(thr[i].wakeat <= now)
			thr[i].state = Ready;
		else if(nearest == 0 || thr[i].wakeat < nearest)
			nearest = thr[i].wakeat;
	}
	if(nearest == 0)
		return 0;
	ms = (nearest - now) / 1000000LL + 1;
	return ms < 1 ? 1 : ms;
}

static uchar iobuf[IOBUF + 4];

static void
iopump(int ms)
{
	int n, tag, i;
	Thr *t;

	n = _iowait(iobuf, sizeof iobuf, ms);
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
		int naptime;

		naptime = wakesleepers();
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
			if(io || naptime > 0){
				iopump(naptime);
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

/* ---- procexec: in a one-instance world, a proc that "becomes" a command
 * is a bare fork plus exec. A runproc's self-directed rfork is intercepted
 * (lib9's _threadrfork hook): RFFDG stashes fds 0..19 high so the parent
 * can restore them after the fork, and RFNAMEG/RFENVG/RFNOTEG are DEFERRED
 * onto the fork itself — the child gets the isolation the runproc meant
 * for it, and the shared instance stays untouched. ---- */

enum { STASHBASE = 100, NSTASH = 20 };
static int deferredflags;
static int stashed;
static ulong stashhad;

static int
threadrfork(int flags)
{
	int i;

	deferredflags |= flags & (RFNAMEG|RFCNAMEG|RFENVG|RFCENVG|RFNOTEG|RFNOMNT);
	if(flags & RFFDG){
		if(stashed)
			sysfatal("procexec: concurrent fd stash");
		stashed = 1;
		stashhad = 0;
		for(i = 0; i < NSTASH; i++)
			if(dup(i, STASHBASE + i) >= 0)
				stashhad |= 1UL << i;
	}
	return 0;
}

static void
fdrestore(void)
{
	int i;

	if(!stashed)
		return;
	for(i = 0; i < NSTASH; i++){
		if(stashhad & (1UL << i))
			dup(STASHBASE + i, i);
		else
			close(i);
		close(STASHBASE + i);
	}
	stashed = 0;
}

void
procexec(Channel *pidc, char *prog, char *args[])
{
	int pid, i, flags;

	flags = RFFDG|RFREND|RFPROC|deferredflags;
	deferredflags = 0;
	pid = rfork(flags);
	if(pid == 0){
		for(i = 0; i < NSTASH; i++)		/* the command must not see the stash */
			close(STASHBASE + i);
		exec(prog, args);
		fprint(2, "procexec: exec %s: %r\n", prog);
		_exits("exec failed");
	}
	fdrestore();
	if(pidc)
		sendul(pidc, pid < 0 ? ~0 : pid);
	threadexits(nil);				/* this proc became the command */
}

void
procexecl(Channel *pidc, char *prog, ...)
{
	char *args[64];
	int n;
	__builtin_va_list a;

	args[0] = prog;
	__builtin_va_start(a, prog);
	for(n = 1; n < 63 && (args[n] = __builtin_va_arg(a, char*)) != nil; n++)
		;
	__builtin_va_end(a);
	args[n] = nil;
	procexec(pidc, prog, args);
}

/* ---- threadwaitchan: Waitmsg* per exited command, via nohang polling ---- */

static Channel *waitchan;

static void
waitproc(void *v)
{
	char buf[512], *fld[5];
	int n, l;
	Waitmsg *w;

	USED(v);
	for(;;){
		n = _rawawait(buf, sizeof buf - 1, 1);
		if(n <= 0){
			threadsleep(50);
			continue;
		}
		buf[n] = 0;
		if(tokenize(buf, fld, nelem(fld)) != 5)
			continue;
		l = strlen(fld[4]) + 1;
		w = malloc(sizeof(Waitmsg) + l);
		if(w == nil)
			continue;
		w->pid = atoi(fld[0]);
		w->time[0] = atoi(fld[1]);
		w->time[1] = atoi(fld[2]);
		w->time[2] = atoi(fld[3]);
		w->msg = (char*)&w[1];
		memmove(w->msg, fld[4], l);
		sendp(waitchan, w);
	}
}

Channel*
threadwaitchan(void)
{
	if(waitchan == nil){
		waitchan = chancreate(sizeof(Waitmsg*), 4);
		threadcreate(waitproc, nil, 8192);
	}
	return waitchan;
}

static int
threadawait(char *s, int n)
{
	int r;

	for(;;){
		r = _rawawait(s, n, 1);
		if(r != 0)
			return r;
		threadsleep(50);
	}
}

/* ---- threadnotify: a chain of handlers over the note machinery ---- */

static int (*onnote[8])(void*, char*);

static void
notechain(void *v, char *msg)
{
	int i;

	for(i = 0; i < nelem(onnote); i++)
		if(onnote[i] != nil && (*onnote[i])(v, msg)){
			noted(NCONT);
			return;
		}
	noted(NDFLT);
}

int
threadnotify(int (*f)(void*, char*), int in)
{
	int i;
	static int registered;

	if(!registered){
		registered = 1;
		notify(notechain);
	}
	if(in){
		for(i = 0; i < nelem(onnote); i++)
			if(onnote[i] == nil || onnote[i] == f){
				onnote[i] = f;
				return 1;
			}
		return 0;
	}
	for(i = 0; i < nelem(onnote); i++)
		if(onnote[i] == f)
			onnote[i] = nil;
	return 1;
}

/* ---- Ref: trivially exclusive on a cooperative scheduler ---- */

void
incref(Ref *r)
{
	r->ref++;
}

long
decref(Ref *r)
{
	return --r->ref;
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
	_threadrfork = threadrfork;
	_threadawait = threadawait;
	_threadsleep = threadsleep;
	threadcreate(tmain, argv, mainstacksize ? mainstacksize : 8192);
	schedule();
	exits(exitsstatus);
}
