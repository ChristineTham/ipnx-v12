/*
 * APE's system call stubs for the wasm32 architecture — what
 * `ape/lib/ap/syscall/genall` generates on the others, one per call in
 * `libc/9syscall/sys.h`, each named `_` and the call's name in capitals
 * (`_OPEN`), with the prototypes `plan9/sys9.h` gives them. As
 * `libc/wasm/sys.c` says of libc's: this machine's trap is a call to the
 * import of the same name, so the stubs are those imports and nothing
 * more, and the calls are the ones that file has.
 *
 * Every stub but `_NOTIFY` and `_NOTED` makes a jump a signal handler
 * asked for on its way back (`notetramp.c`).
 */
#include "../plan9/lib.h"
#include "../plan9/sys9.h"

#define SYS(name) __attribute__((import_module("sys"), import_name(#name)))

SYS(open)	extern int	__open_(const char*, int);
SYS(create)	extern int	__create(char*, int, unsigned long);
SYS(close)	extern int	__close(int);
SYS(pread)	extern long	__pread(int, void*, long, long long);
SYS(pwrite)	extern long	__pwrite(int, const void*, long, long long);
SYS(seek)	extern long long	__seek(int, long long, int);
SYS(dup)	extern int	__dup(int, int);
SYS(pipe)	extern int	__pipe(int*);
SYS(remove)	extern int	__remove(const char*);
SYS(stat)	extern int	__stat_(const char*, unsigned char*, int);
SYS(fstat)	extern int	__fstat(int, unsigned char*, int);
SYS(wstat)	extern int	__wstat(const char*, unsigned char*, int);
SYS(fwstat)	extern int	__fwstat(int, unsigned char*, int);
SYS(bind)	extern int	__bind(const char*, const char*, int);
SYS(mount)	extern int	__mount(int, int, const char*, int, const char*);
SYS(unmount)	extern int	__unmount(const char*, const char*);
SYS(chdir)	extern int	__chdir_(const char*);
SYS(rfork)	extern int	__rfork(int);
SYS(exec)	extern int	__exec(char*, char**);
SYS(exits)	extern void	__exits(char*);
SYS(await)	extern int	__await(char*, int);
SYS(errstr)	extern int	__errstr(char*, unsigned int);
SYS(fversion)	extern int	__fversion(int, int, char*, int);
SYS(fd2path)	extern int	__fd2path(int, char*, int);
SYS(fauth)	extern int	__fauth(int, char*);
SYS(sleep)	extern int	__sleep(long);
SYS(alarm)	extern long	__alarm(unsigned long);
SYS(notify)	extern int	__notify(void*);
SYS(noted)	extern int	__noted(int);
SYS(rendezvous)	extern void*	__rendezvous(void*, void*);
SYS(semacquire)	extern int	__semacquire(long*, int);
SYS(tsemacquire)	extern int	__tsemacquire(long*, unsigned long);
SYS(semrelease)	extern long	__semrelease(long*, long);
/* calls this kernel does not have, answered as `libc/wasm/sys.c` says */
SYS(segbrk)	extern void*	__segbrk(void*, void*);
SYS(segattach)	extern void*	__segattach(int, char*, void*, unsigned long);
SYS(segdetach)	extern int	__segdetach(void*);
SYS(segfree)	extern int	__segfree(void*, unsigned long);
SYS(segflush)	extern int	__segflush(void*, unsigned long);

void	_notejmped(void);

int	_OPEN(const char *f, int m){ int r = __open_(f, m); _notejmped(); return r; }
int	_CREATE(char *f, int m, unsigned long p){ int r = __create(f, m, p); _notejmped(); return r; }
int	_CLOSE(int fd){ int r = __close(fd); _notejmped(); return r; }
long	_PREAD(int fd, void *b, long n, long long o){ long r = __pread(fd, b, n, o); _notejmped(); return r; }
long	_PWRITE(int fd, void *b, long n, long long o){ long r = __pwrite(fd, b, n, o); _notejmped(); return r; }
long long	_SEEK(int fd, long long n, int t){ long long r = __seek(fd, n, t); _notejmped(); return r; }
int	_DUP(int o, int n){ int r = __dup(o, n); _notejmped(); return r; }
int	_PIPE(int *fd){ int r = __pipe(fd); _notejmped(); return r; }
int	_REMOVE(const char *f){ int r = __remove(f); _notejmped(); return r; }
int	_STAT(const char *f, unsigned char *e, int n){ int r = __stat_(f, e, n); _notejmped(); return r; }
int	_FSTAT(int fd, unsigned char *e, int n){ int r = __fstat(fd, e, n); _notejmped(); return r; }
int	_WSTAT(const char *f, unsigned char *e, int n){ int r = __wstat(f, e, n); _notejmped(); return r; }
int	_FWSTAT(int fd, unsigned char *e, int n){ int r = __fwstat(fd, e, n); _notejmped(); return r; }
int	_BIND(const char *n, const char *o, int f){ int r = __bind(n, o, f); _notejmped(); return r; }
int	_MOUNT(int fd, int a, const char *o, int f, const char *s){ int r = __mount(fd, a, o, f, s); _notejmped(); return r; }
int	_UNMOUNT(const char *n, const char *o){ int r = __unmount(n, o); _notejmped(); return r; }
int	_CHDIR(const char *d){ int r = __chdir_(d); _notejmped(); return r; }
int	_RFORK(int f){ int r = __rfork(f); _notejmped(); return r; }
int	_EXEC(char *f, char *a[]){ int r = __exec(f, a); _notejmped(); return r; }
void	_EXITS(char *s){ __exits(s); for(;;); }
int	_AWAIT(char *s, int n){ int r = __await(s, n); _notejmped(); return r; }
int	_ERRSTR(char *s, unsigned int n){ int r = __errstr(s, n); _notejmped(); return r; }
int	_FVERSION(int fd, int m, char *v, int n){ int r = __fversion(fd, m, v, n); _notejmped(); return r; }
int	_FD2PATH(int fd, char *b, int n){ int r = __fd2path(fd, b, n); _notejmped(); return r; }
int	_FAUTH(int fd, char *a){ int r = __fauth(fd, a); _notejmped(); return r; }
int	_SLEEP(long n){ int r = __sleep(n); _notejmped(); return r; }
int	_ALARM(unsigned long n){ int r = __alarm(n); _notejmped(); return r; }
int	_NOTIFY(int (*f)(void*, char*)){ return __notify(f); }
int	_NOTED(int v){ return __noted(v); }
void*	_RENDEZVOUS(void *t, void *v){ void *r = __rendezvous(t, v); _notejmped(); return r; }
int	_SEMACQUIRE(long *a, int b){ int r = __semacquire(a, b); _notejmped(); return r; }
int	_TSEMACQUIRE(long *a, unsigned long ms){ int r = __tsemacquire(a, ms); _notejmped(); return r; }
long	_SEMRELEASE(long *a, long n){ long r = __semrelease(a, n); _notejmped(); return r; }
void*	_SEGBRK(void *a, void *b){ void *r = __segbrk(a, b); _notejmped(); return r; }
void*	_SEGATTACH(int a, char *c, void *v, unsigned long n){ void *r = __segattach(a, c, v, n); _notejmped(); return r; }
int	_SEGDETACH(void *a){ int r = __segdetach(a); _notejmped(); return r; }
int	_SEGFREE(void *a, unsigned long n){ int r = __segfree(a, n); _notejmped(); return r; }
int	_SEGFLUSH(void *a, unsigned long n){ int r = __segflush(a, n); _notejmped(); return r; }
