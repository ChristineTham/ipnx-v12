/* ipnx-v12 poc — guest C library, Plan 9-shaped. Freestanding wasm32. */
typedef unsigned int   uint;
typedef unsigned long  ulong;
typedef unsigned char  uchar;
typedef long long      vlong;
typedef unsigned long long uvlong;
#define nil ((void*)0)
#define USED(x) ((void)(x))

/* open modes, per Plan 9 open(2) */
#define OREAD   0
#define OWRITE  1
#define ORDWR   2
#define OEXEC   3
#define OTRUNC  16

/* rfork flags, verbatim values from <lib9.h>/fork(2) */
#define RFNAMEG  (1<<0)
#define RFENVG   (1<<1)
#define RFFDG    (1<<2)
#define RFNOTEG  (1<<3)
#define RFPROC   (1<<4)
#define RFMEM    (1<<5)
#define RFNOWAIT (1<<6)
#define RFCNAMEG (1<<10)
#define RFCENVG  (1<<11)
#define RFCFDG   (1<<12)
#define RFREND   (1<<13)

enum { NCONT = 0, NDFLT = 1 };            /* noted(2) */
enum { PNPROC = 1, PNGROUP = 2 };         /* postnote(2) */
#define RFNOMNT  (1<<14)

/* bind flags */
#define MREPL   0x0000
#define MBEFORE 0x0001
#define MAFTER  0x0002
#define MCREATE 0x0004

/* system calls */
int   open(char *path, int mode);
int   close(int fd);
long  pread(int fd, void *buf, long n, vlong off);
long  pwrite(int fd, void *buf, long n, vlong off);
long  read(int fd, void *buf, long n);
long  write(int fd, void *buf, long n);
vlong seek(int fd, vlong off, int type);
int   dup(int oldfd, int newfd);
int   bind(char *name, char *old, int flag);
int   newns(char *file);	/* namespace(6) subset: clear, then apply */

/* streaming SHA-256 (FIPS 180-4) — pkg(1) verifies registry fetches with it
 * and guests have 2MB pooled memories, so hashing must never buffer a file */
typedef struct SHA256state {
	ulong h[8];
	uchar buf[64];
	ulong nbuf;
	uvlong len;
} SHA256state;
void  sha256init(SHA256state*);
void  sha256update(SHA256state*, uchar*, ulong);
void  sha256final(SHA256state*, uchar out[32]);
int   unmount(char *name, char *old);
int   notify(void (*f)(void*, char*));
int   noted(int v);
long  alarm(ulong ms);
int   fork(void);
int   postnote(int group, int pid, char *note);
int   getpid(void);
char* strncpy(char *dst, char *src, long n);
int   chdir(char *path);
int   rfork(int flags);
typedef void (*Forkfn)(void*);
int   procrfork(int flags, Forkfn fn, void *arg);   /* the no-asyncify fork+exec path */
int   exec(char *path, char *argv[]);
void  exits(char *msg);		/* real libc's, over atexit; raw floor is _exits */
void  _exits(char *msg);
int   await(char *s, int n);
int   stat(char *path, uchar *edir, int n);
int   fstat(int fd, uchar *edir, int n);
int   sleep(long ms);
int   errstr(char *buf, int n);
int   pipe(int fd[2]);
int   mount(int fd, int afd, char *old, int flag, char *aname);
int   create(char *path, int omode, ulong perm);
int   remove(char *path);
int   chmod(char *path, ulong mode);	/* class B: libc over wstat, never a syscall */
int   chown(char *path, char *uid);

/* file modes */
#define DMDIR    0x80000000
#define DMSETUID 0x00080000	/* 9P2000.u's bit position (docs/identity.md) */

/* library: strings and prints are the REAL Plan 9 sources (libp9.a);
 * these declarations mirror their ABI for our own commands */
long  strlen(char *s);
int   strcmp(char *a, char *b);
int   strncmp(char *a, char *b, ulong n);
char* strcpy(char *dst, char *src);
char* strchr(char *s, int c);
char* strrchr(char *s, int c);
char* strstr(char *s, char *sub);
char* strcat(char *dst, char *src);
char* strdup(char *s);
char* strecpy(char *dst, char *edst, char *src);	/* libp9.a's, real */
int   cistrcmp(char *a, char *b);			/* libp9.a's, real */
void* memcpy(void *dst, void *src, ulong n);
void* memmove(void *dst, void *src, ulong n);
void* memset(void *dst, int c, ulong n);
int   memcmp(void *a, void *b, ulong n);
int   atoi(char *s);
long  atol(char *s);
int   fprint(int fd, char *fmt, ...);
int   print(char *fmt, ...);
char* seprint(char *buf, char *e, char *fmt, ...);
char* vseprint(char *buf, char *e, char *fmt, __builtin_va_list arg);
int   snprint(char *buf, int n, char *fmt, ...);
char* smprint(char *fmt, ...);
void  sysfatal(char *fmt, ...);
extern char *argv0;
void* malloc(ulong n);
void  free(void *p);
void* realloc(void *p, ulong n);
vlong nsec(void);
int   fd2path(int fd, char *buf, int n);
/* 9P2000 stat record: extract length, mode and name (convM2D, abridged) */
uvlong statlen(uchar *edir);
ulong  statmode(uchar *edir);
char*  statname(uchar *edir, char *buf, int n);
