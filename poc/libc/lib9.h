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
int   chdir(char *path);
int   rfork(int flags);
typedef void (*Forkfn)(void*);
int   procrfork(int flags, Forkfn fn, void *arg);   /* the no-asyncify fork+exec path */
int   exec(char *path, char *argv[]);
void  exits(char *msg);
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
int   link9(char *old, char *new);	/* the V12 additions */
int   symlink9(char *target, char *new);
int   readlink9(char *path, char *buf, int n);
int   lstat(char *path, uchar *edir, int n);

/* file modes */
#define DMDIR    0x80000000
#define DMSETUID 0x00080000	/* 9P2000.u's bit position (docs/uid.md) */
#define DMSYMLINK 0x02000000	/* ditto */

/* library */
long  strlen(char *s);
int   strcmp(char *a, char *b);
char* strchr(char *s, int c);
char* strstr(char *s, char *sub);
void* memcpy(void *dst, void *src, ulong n);
void* memset(void *dst, int c, ulong n);
int   vfmt9(char *buf, int nbuf, char *fmt, __builtin_va_list a);
int   fprint(int fd, char *fmt, ...);
int   print(char *fmt, ...);
int   atoi(char *s);
void* malloc(ulong n);
char* strdup(char *s);
int   strncmp(char *a, char *b, ulong n);
char* strcpy(char *dst, char *src);
/* 9P2000 stat record: extract length, mode and name (convM2D, abridged) */
uvlong statlen(uchar *edir);
ulong  statmode(uchar *edir);
char*  statname(uchar *edir, char *buf, int n);
