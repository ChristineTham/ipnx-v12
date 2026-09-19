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
SYS(sleep)	extern int	__sleep(long);
SYS(alarm)	extern long	__alarm(ulong);

int	open(char *f, int m)			{ return __open(f, m); }
int	create(char *f, int m, ulong p)		{ return __create(f, m, p); }
int	close(int fd)				{ return __close(fd); }
long	pread(int fd, void *b, long n, vlong o)	{ return __pread(fd, b, n, o); }
long	pwrite(int fd, void *b, long n, vlong o){ return __pwrite(fd, b, n, o); }
vlong	seek(int fd, vlong n, int t)		{ return __seek(fd, n, t); }
int	dup(int o, int n)			{ return __dup(o, n); }
int	pipe(int *fd)				{ return __pipe(fd); }
int	remove(char *f)				{ return __remove(f); }
int	stat(char *f, uchar *e, int n)		{ return __stat(f, e, n); }
int	fstat(int fd, uchar *e, int n)		{ return __fstat(fd, e, n); }
int	wstat(char *f, uchar *e, int n)		{ return __wstat(f, e, n); }
int	fwstat(int fd, uchar *e, int n)		{ return __fwstat(fd, e, n); }
int	bind(char *n, char *o, int f)		{ return __bind(n, o, f); }
int	mount(int fd, int a, char *o, int f, char *s) { return __mount(fd, a, o, f, s); }
int	unmount(char *n, char *o)		{ return __unmount(n, o); }
int	chdir(char *d)				{ return __chdir(d); }
int	rfork(int f)				{ return __rfork(f); }
int	exec(char *f, char *a[])		{ return __exec(f, a); }
int	await(char *s, int n)			{ return __await(s, n); }
int	errstr(char *s, uint n)			{ return __errstr(s, n); }
int	fversion(int fd, int m, char *v, int n)	{ return __fversion(fd, m, v, n); }
int	sleep(long n)				{ return __sleep(n); }
long	alarm(ulong n)				{ return __alarm(n); }

/*
 * `exits` is the one call whose stub is not a forwarding call: it does not
 * return, and the compiler must know that or it will emit unreachable code
 * after every call to it. Plan 9's 386 stub is named `_exits` for a different
 * reason (`9syscall/mkfile`: "if(~ $i exits) i=_exits") — there, `exits` is a
 * portable wrapper that flushes atexit handlers first.
 */
void
exits(char *status)
{
	__exits(status);
	for(;;)
		;
}

void
_exits(char *status)
{
	exits(status);
}
