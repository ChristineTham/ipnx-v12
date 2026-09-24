/*
 * The system call stubs for the wasm32 architecture.
 *
 * Plan 9 generates one of these per architecture from a single table:
 * `libc/9syscall/sys.h` holds the call numbers, and `libc/9syscall/mkfile`
 * emits the assembly — for 386, `MOVL $n, AX; INT $64; RET`. The stub is
 * whatever that machine's trap instruction is, and nothing more.
 *
 * This machine has no trap instruction and no registers. Its calls are module
 * imports, resolved by the embedding when the module is instantiated, so the
 * table is the import list below and the "trap" is a call. That is the whole
 * of the difference, and it is the same difference `main9.c` records: where
 * Plan 9's arch directory holds assembly, this one holds imports.
 *
 * The names are Plan 9's, from `sys.h`, and every signature is the one in
 * `plan9/sys/include/libc.h`.
 */
#include <u.h>
#include <libc.h>

#define SYS(name) __attribute__((import_module("sys"), import_name(#name)))

SYS(open)	extern int	__open(char*, int);
SYS(create)	extern int	__create(char*, int, ulong);
SYS(close)	extern int	__close(int);
SYS(pread)	extern long	__pread(int, void*, long, vlong);
SYS(pwrite)	extern long	__pwrite(int, void*, long, vlong);
SYS(seek)	extern vlong	__seek(int, vlong, int);
SYS(dup)	extern int	__dup(int, int);
SYS(pipe)	extern int	__pipe(int*);
SYS(remove)	extern int	__remove(char*);
SYS(stat)	extern int	__stat(char*, uchar*, int);
SYS(fstat)	extern int	__fstat(int, uchar*, int);
SYS(wstat)	extern int	__wstat(char*, uchar*, int);
SYS(fwstat)	extern int	__fwstat(int, uchar*, int);
SYS(bind)	extern int	__bind(char*, char*, int);
SYS(mount)	extern int	__mount(int, int, char*, int, char*);
SYS(unmount)	extern int	__unmount(char*, char*);
SYS(chdir)	extern int	__chdir(char*);
SYS(rfork)	extern int	__rfork(int);
SYS(exec)	extern int	__exec(char*, char**);
SYS(exits)	extern void	__exits(char*);
SYS(await)	extern int	__await(char*, int);
SYS(errstr)	extern int	__errstr(char*, uint);
SYS(fversion)	extern int	__fversion(int, int, char*, int);
SYS(fd2path)	extern int	__fd2path(int, char*, int);
SYS(sleep)	extern int	__sleep(long);
SYS(alarm)	extern long	__alarm(ulong);
/*
 * `notify`, `noted` and `rendezvous` — NOTIFY is call 28 in
 * `libc/9syscall/sys.h`, and Plan 9 generates their stubs from that file
 * with no C source at all. They are declared here for the same reason the
 * others are. A handler is entered through `__notestart` (procrfork.c).
 */
SYS(notify)	extern int	__notify(void*);
SYS(noted)	extern int	__noted(int);
SYS(rendezvous)	extern void*	__rendezvous(void*, void*);
/*
 * SEMACQUIRE 37, SEMRELEASE 38, TSEMACQUIRE 52 — calls, not a library:
 * the kernel reads and swaps the word at the address it is given.
 */
SYS(semacquire)	extern int	__semacquire(long*, int);
SYS(tsemacquire)	extern int	__tsemacquire(long*, ulong);
SYS(semrelease)	extern long	__semrelease(long*, long);

/*
 * Every stub but `notify` and `noted` looks, on the way back, for a jump a
 * note handler asked for with `notejmp` — the one place on this machine
 * where the process's own frames are the innermost again (`notejmp.c`).
 */
void	_notejmped(void);

int	open(char *f, int m){ int r = __open(f, m); _notejmped(); return r; }
int	create(char *f, int m, ulong p){ int r = __create(f, m, p); _notejmped(); return r; }
int	close(int fd){ int r = __close(fd); _notejmped(); return r; }
long	pread(int fd, void *b, long n, vlong o){ long r = __pread(fd, b, n, o); _notejmped(); return r; }
long	pwrite(int fd, void *b, long n, vlong o){ long r = __pwrite(fd, b, n, o); _notejmped(); return r; }
vlong	seek(int fd, vlong n, int t){ vlong r = __seek(fd, n, t); _notejmped(); return r; }
int	dup(int o, int n){ int r = __dup(o, n); _notejmped(); return r; }
int	pipe(int *fd){ int r = __pipe(fd); _notejmped(); return r; }
int	remove(char *f){ int r = __remove(f); _notejmped(); return r; }
int	stat(char *f, uchar *e, int n){ int r = __stat(f, e, n); _notejmped(); return r; }
int	fstat(int fd, uchar *e, int n){ int r = __fstat(fd, e, n); _notejmped(); return r; }
int	wstat(char *f, uchar *e, int n){ int r = __wstat(f, e, n); _notejmped(); return r; }
int	fwstat(int fd, uchar *e, int n){ int r = __fwstat(fd, e, n); _notejmped(); return r; }
int	bind(char *n, char *o, int f){ int r = __bind(n, o, f); _notejmped(); return r; }
int	mount(int fd, int a, char *o, int f, char *s){ int r = __mount(fd, a, o, f, s); _notejmped(); return r; }
int	unmount(char *n, char *o){ int r = __unmount(n, o); _notejmped(); return r; }
int	chdir(char *d){ int r = __chdir(d); _notejmped(); return r; }
int	rfork(int f){ int r = __rfork(f); _notejmped(); return r; }
int	exec(char *f, char *a[]){ int r = __exec(f, a); _notejmped(); return r; }
int	await(char *s, int n){ int r = __await(s, n); _notejmped(); return r; }
int	errstr(char *s, uint n){ int r = __errstr(s, n); _notejmped(); return r; }
int	fversion(int fd, int m, char *v, int n){ int r = __fversion(fd, m, v, n); _notejmped(); return r; }
int	fd2path(int fd, char *b, int n){ int r = __fd2path(fd, b, n); _notejmped(); return r; }
int	sleep(long n){ int r = __sleep(n); _notejmped(); return r; }
long	alarm(ulong n){ long r = __alarm(n); _notejmped(); return r; }
int	notify(void (*f)(void*, char*))		{ return __notify(f); }
int	noted(int v)				{ return __noted(v); }
void*	rendezvous(void *tag, void *val){ void* r = __rendezvous(tag, val); _notejmped(); return r; }
int	semacquire(long *a, int b){ int r = __semacquire(a, b); _notejmped(); return r; }
int	tsemacquire(long *a, ulong ms){ int r = __tsemacquire(a, ms); _notejmped(); return r; }
long	semrelease(long *a, long n){ long r = __semrelease(a, n); _notejmped(); return r; }

/*
 * `exits` is the one call whose stub is not a forwarding call. Two things
 * differ, and Plan 9 has both:
 *
 *   * it does not return, and the compiler must know that or it emits
 *     unreachable code after every call to it;
 *   * **the stub is named `_exits`** — `9syscall/mkfile`: `if(~ $i exits)
 *     i=_exits` — because `exits` is a PORTABLE function
 *     (`port/atexit.c:46`) that runs the `atexit` handlers and then calls
 *     this. Defining `exits` here would take that over and every handler
 *     would be silently skipped, `Bflush` among them.
 */
void
_exits(char *status)
{
	__exits(status);
	for(;;)
		;
}
