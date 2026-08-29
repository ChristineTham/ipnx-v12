/* lib9: syscall stubs + the slice of libc the commands need.
 * Trap numbers are Plan 9's own, from /sys/src/libc/9syscall/sys.h. */
#include "lib9.h"

__attribute__((import_module("env"), import_name("sys")))
extern int _sysimp(int trap, int a, int b, int c, int d, int e);
int _sys(int t, int a, int b, int c, int d, int e){ return _sysimp(t,a,b,c,d,e); }

/* The fork guard: a hand-assembled wasm function whose try_table/catch_all
 * frame is what the child's exec unwinds back to (RESEARCH.md §5.2). The
 * catch frame must be live when the child execs, so the child's pre-exec
 * code runs as a function called inside the guard's dynamic extent — the
 * thread-library shape (procrfork), not bare dual-return rfork. */
__attribute__((import_module("guard"), import_name("rfork")))
extern int _rforkguard(int flags, int sp, int fn, int arg);

/* Called back from the host, inside the guard's extent, to run the child. */
__attribute__((export_name("__forkshim")))
void __forkshim(int fn, int arg){
	((Forkfn)fn)((void*)arg);
	exits("procrfork: child returned");
}

enum { BIND=2, CHDIR=3, CLOSE=4, DUP=5, EXEC=7, EXITS=8, OPEN=14, SLEEP=17,
       RFORK=19, PIPE=21, CREATE=22, REMOVE=25, SEEK=39, ERRSTR=41,
       STAT=42, FSTAT=43, WSTAT=44, MOUNT=46, AWAIT=47, PREAD=50, PWRITE=51,
       LINK=60, SYMLINK=61, READLINK=62 };

int open(char *p, int m)          { return _sys(OPEN,(int)p,m,0,0,0); }
int close(int fd)                 { return _sys(CLOSE,fd,0,0,0,0); }
long (*_threadpread)(int, void*, long, vlong);	/* libthread's yielding read */
long pread(int fd, void *b, long n, vlong off){
	if(_threadpread)
		return (*_threadpread)(fd, b, n, off);
	return _sys(PREAD,fd,(int)b,n,(int)(off&0xffffffff),(int)(off>>32)); }
long _rawpread(int fd, void *b, long n, vlong off){
	return _sys(PREAD,fd,(int)b,n,(int)(off&0xffffffff),(int)(off>>32)); }
int _aread(int tag, int fd, int n){ return _sys(210, tag, fd, n, 0, 0); }
int _iowait(void *buf, int n, int ms){ return _sys(211, (int)buf, n, ms, 0, 0); }
long pwrite(int fd, void *b, long n, vlong off){
	return _sys(PWRITE,fd,(int)b,n,(int)(off&0xffffffff),(int)(off>>32)); }
long read(int fd, void *b, long n) { return pread(fd,b,n,-1LL); }
long write(int fd, void *b, long n){ return pwrite(fd,b,n,-1LL); }
vlong seek(int fd, vlong off, int t){
	return _sys(SEEK,fd,(int)(off&0xffffffff),(int)(off>>32),t,0); }
int dup(int o, int n)             { return _sys(DUP,o,n,0,0,0); }
int bind(char *nm, char *old, int f){ return _sys(BIND,(int)nm,(int)old,f,0,0); }
int chdir(char *p)                { return _sys(CHDIR,(int)p,0,0,0,0); }
int exec(char *p, char *argv[])   { return _sys(EXEC,(int)p,(int)argv,0,0,0); }
void _exits(char *msg)            { _sys(EXITS,(int)msg,0,0,0,0); for(;;); }
int (*_threadawait)(char*, int);	/* libthread's polling await */
int await(char *s, int n){
	if(_threadawait)
		return (*_threadawait)(s, n);
	return _sys(AWAIT,(int)s,n,0,0,0);
}
int _rawawait(char *s, int n, int nohang){ return _sys(AWAIT,(int)s,n,nohang,0,0); }
__attribute__((weak)) int stat(char *p, uchar *e, int n){ return _sys(STAT,(int)p,(int)e,n,0,0); }
int lstat(char *p, uchar *e, int n){ return _sys(STAT,(int)p,(int)e,n,1,0); }
int link9(char *old, char *new){ return _sys(LINK,(int)old,(int)new,0,0,0); }
int symlink9(char *t, char *new){ return _sys(SYMLINK,(int)t,(int)new,0,0,0); }
int readlink9(char *p, char *b, int n){ return _sys(READLINK,(int)p,(int)b,n,0,0); }
__attribute__((weak)) int fstat(int fd, uchar *e, int n){ return _sys(FSTAT,fd,(int)e,n,0,0); }
int (*_threadsleep)(long);	/* libthread parks the thread, not the process */
int sleep(long ms){
	if(_threadsleep)
		return (*_threadsleep)(ms);
	return _sys(SLEEP,ms,0,0,0,0);
}
int errstr(char *b, int n)        { return _sys(ERRSTR,(int)b,n,0,0,0); }
int pipe(int fd[2])               { return _sys(PIPE,(int)fd,0,0,0,0); }
int mount(int fd, int afd, char *old, int flag, char *aname){
	return _sys(MOUNT,fd,afd,(int)old,flag,(int)aname); }

/* chmod/chown: one wstat(5) record with don't-touch values everywhere else —
 * the class-B shape, forty lines for the family, no syscalls added. */
static uchar *wput(uchar *p, int nb, uvlong v){
	int i;
	for(i = 0; i < nb; i++)
		*p++ = (uchar)(v >> (8*i));
	return p;
}
static int
wstat9(char *path, ulong mode, char *uid)
{
	uchar rec[128], *p, *sz;
	int i;

	sz = rec;
	p = wput(rec, 2, 0);			/* size, patched */
	p = wput(p, 2, 0xFFFF);			/* type: don't touch */
	p = wput(p, 4, 0xFFFFFFFFUL);		/* dev */
	for(i = 0; i < 13; i++)			/* qid */
		*p++ = 0xFF;
	p = wput(p, 4, mode);
	p = wput(p, 4, 0xFFFFFFFFUL);		/* atime */
	p = wput(p, 4, 0xFFFFFFFFUL);		/* mtime */
	p = wput(p, 8, ~(uvlong)0);		/* length */
	p = wput(p, 2, 0);			/* name: don't touch */
	if(uid == nil)
		p = wput(p, 2, 0);
	else{
		p = wput(p, 2, strlen(uid));
		memcpy(p, uid, strlen(uid));
		p += strlen(uid);
	}
	p = wput(p, 2, 0);			/* gid */
	p = wput(p, 2, 0);			/* muid */
	wput(sz, 2, (p - rec) - 2);
	return _sys(WSTAT, (int)path, (int)rec, p - rec, 0, 0);
}
int chmod(char *path, ulong mode){ return wstat9(path, mode, nil); }
int chown(char *path, char *uid){ return wstat9(path, 0xFFFFFFFFUL, uid); }
int create(char *p, int m, ulong perm){ return _sys(CREATE,(int)p,m,(int)perm,0,0); }
int remove(char *p)               { return _sys(REMOVE,(int)p,0,0,0,0); }

/* Bare dual-return rfork rides asyncify (RESEARCH §5.2): the worker unwinds
 * this process's stack into _asydata, copies the whole linear memory, and
 * rewinds both copies — pid into this one, 0 into the other. Only binaries
 * transformed by wasm-opt --asyncify (mk.sh's ASYNCIFY list) can take it. */
__attribute__((import_module("env"), import_name("forka")))
extern int _forka(int flags, int databuf);

/* Real setjmp/longjmp ride the same machinery (guestcore's setj/longj):
 * only asyncified binaries can take them — sam does. The jmp_buf is only
 * a key; the saved frames live host-side. */
__attribute__((import_module("env"), import_name("setj")))
extern int _setj(int env);
__attribute__((import_module("env"), import_name("longj")))
extern int _longj(int env, int val);
__attribute__((import_module("env"), import_name("sjbuf")))
extern void _sjbuf(int p);
__attribute__((import_module("env"), import_name("tsave")))
extern int _tsave(int id);
__attribute__((import_module("env"), import_name("tjump")))
extern int _tjump(int id, int val);
__attribute__((import_module("env"), import_name("tdrop")))
extern void _tdrop(int id);

static uchar _asydata[8 + 32768];

int (*_threadrfork)(int);	/* libthread intercepts self-directed rfork */
int rfork(int flags){
	if(flags & RFPROC){
		*(uint*)_asydata = (uint)(_asydata + 8);
		*((uint*)_asydata + 1) = (uint)(_asydata + sizeof _asydata);
		return _forka(flags, (int)_asydata);
	}
	if(_threadrfork)
		return (*_threadrfork)(flags);
	return _sys(RFORK, flags, 0, 0, 0, 0);
}

int procrfork(int flags, Forkfn fn, void *arg){
	return _rforkguard(flags|RFPROC|RFMEM,
		(int)__builtin_frame_address(0), (int)fn, (int)arg);
}

/* ---- core the real libraries sit on ---- */

char *argv0;		/* set by ARGBEGIN; Plan 9's startup owns it, so ours does */

/* malloc: first-fit free list over memory.grow — enough for the real
 * userspace; Plan 9's pool allocator can come later if it matters */
typedef struct Hdr Hdr;
struct Hdr { ulong size; Hdr *next; };
static Hdr *freelist;
__attribute__((export_name("__freelist")))
int __freelist(void){ return (int)&freelist; }
extern char __heap_base;
static char *hp;

static void *
morecore(ulong n)
{
	char *r;

	if(hp == nil)
		hp = &__heap_base;
	while(hp+n > (char*)(__builtin_wasm_memory_size(0)*65536UL))
		if(__builtin_wasm_memory_grow(0, 16) == (ulong)-1)
			return nil;
	r = hp;
	hp += n;
	return r;
}

void *
malloc(ulong n)
{
	Hdr *h, **pp;

	n = ((n+7)&~7UL) + sizeof(Hdr);
	for(pp = (Hdr**)&freelist; (h = *pp) != nil; pp = &h->next)
		if(h->size >= n){
			*pp = h->next;
			return h+1;
		}
	h = morecore(n < 4096 ? 4096 : n);
	if(h == nil)
		return nil;
	h->size = n < 4096 ? 4096 : n;
	return h+1;
}

void
free(void *p)
{
	Hdr *h;

	if(p == nil)
		return;
	h = (Hdr*)p - 1;
	h->next = freelist;
	freelist = h;
}

void *
realloc(void *p, ulong n)
{
	Hdr *h;
	void *q;
	ulong old;

	if(p == nil)
		return malloc(n);
	h = (Hdr*)p - 1;
	old = h->size - sizeof(Hdr);
	if(old >= n)
		return p;
	q = malloc(n);
	if(q != nil){
		memmove(q, p, old);
		free(p);
	}
	return q;
}

void *
mallocz(ulong n, int clr)
{
	char *p = malloc(n);
	ulong i;

	if(p != nil && clr)
		for(i = 0; i < n; i++)
			p[i] = 0;
	return p;
}

void *calloc(ulong n, ulong m){ return mallocz(n*m, 1); }
void *sbrk(ulong n){ return morecore(n); }	/* grep brings its own allocator */
void setmalloctag(void *p, ulong t){ USED(p); USED(t); }
void setrealloctag(void *p, ulong t){ USED(p); USED(t); }
ulong getmalloctag(void *p){ USED(p); return 0; }
ulong getcallerpc(void *p){ USED(p); return 0; }
void lock(void *l){ USED(l); }		/* one thread per process */
void unlock(void *l){ USED(l); }
int canlock(void *l){ USED(l); return 1; }
int _efgfmt(void *f){ USED(f); return -1; }	/* floats: not on this port yet */
void _assert(char *s){ fprint(2, "assert failed: %s\n", s); exits("assert"); }
/* Real setjmp: unwind via asyncify, save the frames host-side, rewind in
 * place returning 0; longjmp restores those frames and rewinds with the
 * value. The fork machinery's exact dance, keyed by the env pointer. */
int setjmp(long *env){
	*(uint*)_asydata = (uint)(_asydata + 8);
	*((uint*)_asydata + 1) = (uint)(_asydata + sizeof _asydata);
	_sjbuf((int)_asydata);
	return _setj((int)env);
}
/* the rune-range binary search runetype.c leans on — element type MUST
 * match the platform Rune (u.h: unsigned int, the 21-bit era): the callers
 * pass Rune tables, and a narrower type here half-strides them silently
 * (wasm promotes both to i32, so the link never complains — measured
 * 2026-08-29 as wc counting zero words) */
unsigned int *_runebsearch(unsigned int c, unsigned int *t, int n, int ne){
	unsigned int *p;
	int m;

	while(n > 1){
		m = n/2;
		p = t + m*ne;
		if(c >= p[0]){
			t = p;
			n = n-m;
		} else
			n = m;
	}
	if(n && c >= t[0])
		return t;
	return 0;
}
/* Notes: the kernel queues them and flags the mailbox; guestcore calls
 * __notedispatch at the next syscall return (V7's timing, a recorded
 * deviation from Plan 9's anytime delivery). The handler must call noted():
 * NCONT returns here and the interrupted call has already returned -1 with
 * errstr "interrupted"; NDFLT never returns (the kernel kills mid-syscall). */
static void (*_onnote)(void*, char*);
int notify(void (*f)(void*, char*)){
	_onnote = f;
	return _sys(28, f != 0, 0, 0, 0, 0);
}
int noted(int v){ return _sys(29, v, 0, 0, 0, 0); }
__attribute__((export_name("__notedispatch")))
void __notedispatch(void){
	char note[128];

	while(_sys(202, (int)note, sizeof note, 0, 0, 0) > 0){
		if(_onnote == 0)
			noted(NDFLT);
		else
			(*_onnote)(nil, note);
	}
}
long alarm(ulong ms)              { return _sys(6, (int)ms, 0, 0, 0, 0); }
/* qlocks: guests are single-threaded until libthread lands, so holding is
 * trivially exclusive — but the calls must exist for the real libraries */
void qlock(void *q)               { USED(q); }
void qunlock(void *q)             { USED(q); }
int canqlock(void *q)             { USED(q); return 1; }
/* port/execl.c reads &f+1 — a stack-varargs assumption wasm cannot honour
 * (variadic args travel in a separate buffer), so this one is ours */
int execl(char *f, ...){
	char *argv[64];
	int n;
	__builtin_va_list a;

	__builtin_va_start(a, f);
	for(n = 0; n < 63 && (argv[n] = __builtin_va_arg(a, char*)) != nil; n++)
		;
	__builtin_va_end(a);
	argv[n] = nil;
	return exec(f, argv);
}
int unmount(char *name, char *old){ return _sys(35, (int)name, (int)old, 0, 0, 0); }
int fork(void)                    { return rfork(RFFDG|RFREND|RFPROC); }
/* Thread contexts (libthread's coroutines): setjmp/longjmp plus the
 * shadow-stack region, so coroutines can share stack addresses. */
int _tsavec(int id){
	*(uint*)_asydata = (uint)(_asydata + 8);
	*((uint*)_asydata + 1) = (uint)(_asydata + sizeof _asydata);
	_sjbuf((int)_asydata);
	return _tsave(id);
}
void _tjumpc(int id, int v){
	*(uint*)_asydata = (uint)(_asydata + 8);
	*((uint*)_asydata + 1) = (uint)(_asydata + sizeof _asydata);
	_sjbuf((int)_asydata);
	_tjump(id, v);
	for(;;);
}
void _tdropc(int id){ _tdrop(id); }
void longjmp(long *env, int v){
	*(uint*)_asydata = (uint)(_asydata + 8);
	*((uint*)_asydata + 1) = (uint)(_asydata + sizeof _asydata);
	_sjbuf((int)_asydata);
	_longj((int)env, v == 0 ? 1 : v);
	for(;;);
}
void abort(void){ exits("abort"); }

vlong nsec(void){
	vlong v;

	if(_sys(53, (int)&v, 0, 0, 0, 0) != 8)
		return -1;
	return v;
}
int fd2path(int fd, char *buf, int n){ return _sys(23, fd, (int)buf, n, 0, 0) >= 0 ? 0 : -1; }   /* 0 on success, per fd2path(2) */
int wstat(char *p, uchar *e, int n){ return _sys(44, (int)p, (int)e, n, 0, 0); }
int fwstat(int fd, uchar *e, int n){ return _sys(45, fd, (int)e, n, 0, 0); }

/* 9P2000 stat(5) record, abridged read: size[2] type[2] dev[4] qid[13]
 * mode[4] atime[4] mtime[4] length[8] name[s] ... */
static uvlong gle(uchar *p, int n){
	uvlong v=0; int i; for(i=n-1;i>=0;i--) v=(v<<8)|p[i]; return v; }
uvlong statlen(uchar *e){ return gle(e+2+2+4+13+4+4+4, 8); }
ulong statmode(uchar *e){ return (ulong)gle(e+2+2+4+13, 4); }
char *statname(uchar *e, char *buf, int n){
	uchar *p=e+2+2+4+13+4+4+4+8; int m=(int)gle(p,2);
	if(m>n-1) m=n-1;
	memcpy(buf,p+2,m); buf[m]=0; return buf; }

/* umuldiv is per-arch assembler upstream; wasm has 64-bit integers */
ulong
umuldiv(ulong a, ulong b, ulong c)
{
	return ((uvlong)a * b) / c;
}

/* ---- SHA-256, streaming (FIPS 180-4) ---- */
static ulong sha_k[64] = {
	0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
	0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
	0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
	0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
	0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
	0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
	0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
	0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
};
#define ROR(x,n) (((x)>>(n)) | ((x)<<(32-(n))))
static void
sha_block(SHA256state *s, uchar *p)
{
	ulong w[64], a,b,c,d,e,f,g,h,t1,t2;
	int i;

	for(i = 0; i < 16; i++)
		w[i] = (p[i*4]<<24) | (p[i*4+1]<<16) | (p[i*4+2]<<8) | p[i*4+3];
	for(; i < 64; i++){
		ulong s0 = ROR(w[i-15],7) ^ ROR(w[i-15],18) ^ (w[i-15]>>3);
		ulong s1 = ROR(w[i-2],17) ^ ROR(w[i-2],19) ^ (w[i-2]>>10);
		w[i] = w[i-16] + s0 + w[i-7] + s1;
	}
	a=s->h[0]; b=s->h[1]; c=s->h[2]; d=s->h[3];
	e=s->h[4]; f=s->h[5]; g=s->h[6]; h=s->h[7];
	for(i = 0; i < 64; i++){
		t1 = h + (ROR(e,6)^ROR(e,11)^ROR(e,25)) + ((e&f)^(~e&g)) + sha_k[i] + w[i];
		t2 = (ROR(a,2)^ROR(a,13)^ROR(a,22)) + ((a&b)^(a&c)^(b&c));
		h=g; g=f; f=e; e=d+t1; d=c; c=b; b=a; a=t1+t2;
	}
	s->h[0]+=a; s->h[1]+=b; s->h[2]+=c; s->h[3]+=d;
	s->h[4]+=e; s->h[5]+=f; s->h[6]+=g; s->h[7]+=h;
}
void
sha256init(SHA256state *s)
{
	s->h[0]=0x6a09e667; s->h[1]=0xbb67ae85; s->h[2]=0x3c6ef372; s->h[3]=0xa54ff53a;
	s->h[4]=0x510e527f; s->h[5]=0x9b05688c; s->h[6]=0x1f83d9ab; s->h[7]=0x5be0cd19;
	s->nbuf = 0;
	s->len = 0;
}
void
sha256update(SHA256state *s, uchar *p, ulong n)
{
	s->len += n;
	while(n > 0){
		if(s->nbuf == 0 && n >= 64){
			sha_block(s, p);
			p += 64; n -= 64;
			continue;
		}
		while(s->nbuf < 64 && n > 0){
			s->buf[s->nbuf++] = *p++;
			n--;
		}
		if(s->nbuf == 64){
			sha_block(s, s->buf);
			s->nbuf = 0;
		}
	}
}
void
sha256final(SHA256state *s, uchar out[32])
{
	uvlong bits = s->len * 8;
	uchar pad = 0x80;
	uchar zero = 0;
	uchar lb[8];
	int i;

	sha256update(s, &pad, 1);
	while(s->nbuf != 56)
		sha256update(s, &zero, 1);
	for(i = 0; i < 8; i++)
		lb[i] = bits >> (56 - i*8);
	sha256update(s, lb, 8);
	for(i = 0; i < 8; i++){
		out[i*4]   = s->h[i] >> 24;
		out[i*4+1] = s->h[i] >> 16;
		out[i*4+2] = s->h[i] >> 8;
		out[i*4+3] = s->h[i];
	}
}
