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
       STAT=42, FSTAT=43, AWAIT=47, PREAD=50, PWRITE=51 };

int open(char *p, int m)          { return _sys(OPEN,(int)p,m,0,0,0); }
int close(int fd)                 { return _sys(CLOSE,fd,0,0,0,0); }
long pread(int fd, void *b, long n, vlong off){
	return _sys(PREAD,fd,(int)b,n,(int)(off&0xffffffff),(int)(off>>32)); }
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
void exits(char *msg)             { _sys(EXITS,(int)msg,0,0,0,0); for(;;); }
int await(char *s, int n)         { return _sys(AWAIT,(int)s,n,0,0,0); }
int stat(char *p, uchar *e, int n){ return _sys(STAT,(int)p,(int)e,n,0,0); }
int fstat(int fd, uchar *e, int n){ return _sys(FSTAT,fd,(int)e,n,0,0); }
int sleep(long ms)                { return _sys(SLEEP,ms,0,0,0,0); }
int errstr(char *b, int n)        { return _sys(ERRSTR,(int)b,n,0,0,0); }
int pipe(int fd[2])               { return _sys(PIPE,(int)fd,0,0,0,0); }
int create(char *p, int m, ulong perm){ return _sys(CREATE,(int)p,m,(int)perm,0,0); }
int remove(char *p)               { return _sys(REMOVE,(int)p,0,0,0,0); }

int rfork(int flags){
	return _sys(RFORK, flags, 0, 0, 0, 0);   /* bare RFPROC: the kernel says why not */
}

int procrfork(int flags, Forkfn fn, void *arg){
	return _rforkguard(flags|RFPROC|RFMEM,
		(int)__builtin_frame_address(0), (int)fn, (int)arg);
}

/* ---- library ---- */
long strlen(char *s){ char *p=s; while(*p) p++; return p-s; }
int strcmp(char *a, char *b){
	for(; *a && *a==*b; a++, b++); return (uchar)*a - (uchar)*b; }
char *strchr(char *s, int c){
	for(; *s; s++) if(*s==c) return s; return c==0?s:nil; }
char *strstr(char *s, char *sub){
	long n=strlen(sub);
	for(; *s; s++){ char *a=s,*b=sub; long i;
		for(i=0;i<n && a[i]==b[i];i++); if(i==n) return s; }
	return nil; }
void *memcpy(void *d, void *s, ulong n){
	uchar *dp=d, *sp=s; while(n--) *dp++=*sp++; return d; }
void *memset(void *d, int c, ulong n){
	uchar *p=d; while(n--) *p++=c; return d; }
int atoi(char *s){ int v=0,neg=0; if(*s=='-'){neg=1;s++;}
	while(*s>='0'&&*s<='9') v=v*10+*s++-'0'; return neg?-v:v; }
int strncmp(char *a, char *b, ulong n){
	for(; n && *a && *a==*b; a++, b++, n--)
		;
	return n ? (uchar)*a - (uchar)*b : 0; }
char *strcpy(char *d, char *s){ char *r=d; while((*d++=*s++))
		; return r; }

/* bump allocator; nothing is ever freed (v0 — documented) */
extern char __heap_base;
void *malloc(ulong n){
	static char *hp;
	char *r;

	if(hp == nil)
		hp = &__heap_base;
	n = (n+7)&~7UL;
	while(hp+n > (char*)(__builtin_wasm_memory_size(0)*65536UL))
		if(__builtin_wasm_memory_grow(0, 16) == (ulong)-1)
			exits("out of memory");
	r = hp;
	hp += n;
	return r;
}
char *strdup(char *s){ char *r=malloc(strlen(s)+1); strcpy(r,s); return r; }
/* clang -O2 rewrites malloc+memset pairs into calloc; satisfy it (byte loop
 * rather than memset so the pattern cannot reappear inside) */
void *calloc(ulong n, ulong m){
	char *p = malloc(n*m); ulong i;
	for(i = 0; i < n*m; i++) p[i] = 0;
	return p;
}

/* fprint: %s %d %x %c %r and %% — all the tests need */
static char *fmtnum(char *p, unsigned v, int base, int neg){
	char t[16]; int i=0;
	if(neg) *p++='-';
	do{ t[i++]="0123456789abcdef"[v%base]; v/=base; }while(v);
	while(i) *p++=t[--i];
	return p;
}
static int vfmt(char *buf, int nbuf, char *fmt, __builtin_va_list a){
	char *p=buf, *e=buf+nbuf-1, *s;
	for(; *fmt && p<e; fmt++){
		if(*fmt!='%'){ *p++=*fmt; continue; }
		switch(*++fmt){
		case 's': s=__builtin_va_arg(a,char*); if(s==nil)s="<nil>";
			while(*s && p<e) *p++=*s++; break;
		case 'd': { int v=__builtin_va_arg(a,int);
			p=fmtnum(p, v<0?-(unsigned)v:(unsigned)v, 10, v<0); break; }
		case 'x': p=fmtnum(p, __builtin_va_arg(a,unsigned), 16, 0); break;
		case 'c': *p++=(char)__builtin_va_arg(a,int); break;
		case 'r': { char eb[128]; errstr(eb,sizeof eb); s=eb;
			while(*s && p<e) *p++=*s++; break; }
		case '%': *p++='%'; break;
		default: *p++='%'; if(p<e) *p++=*fmt;
		}
	}
	*p=0;
	return p-buf;
}
int fprint(int fd, char *fmt, ...){
	char buf[512]; int n;
	__builtin_va_list a; __builtin_va_start(a,fmt);
	n=vfmt(buf,sizeof buf,fmt,a); __builtin_va_end(a);
	return write(fd,buf,n);
}
int print(char *fmt, ...){
	char buf[512]; int n;
	__builtin_va_list a; __builtin_va_start(a,fmt);
	n=vfmt(buf,sizeof buf,fmt,a); __builtin_va_end(a);
	return write(1,buf,n);
}

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
