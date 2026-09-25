/*
 * `setjmp`, `longjmp` and `sigsetjmp` — `ape/lib/ap/386/setjmp.s`. The
 * first two are the machine's, as `libc/wasm/setjmp.c` says: the stack is
 * unwound into `asyncbuf` and kept under the jmp_buf's address. APE's
 * jmp_buf is four words (`<setjmp.h>`); these use the first two, SP and pc,
 * as setjmp.s does. `sigsetjmp` is 386's: the mask flag in `j[0]`,
 * `$_psigblocked` in `j[1]` — its ADDRESS, as every architecture's stores
 * it (`amd64/setjmp.s:23`, *"this seemed wrong … current code matches all
 * other archs' impl'ns"*) — and a setjmp into `j+2`.
 */
#include <setjmp.h>
#include <signal.h>

#define SYS(name) __attribute__((import_module("sys"), import_name(#name)))

SYS(setjmp)	extern int	__setjmp(uintptr_t*);
SYS(longjmp)	extern void	__longjmp(uintptr_t*, int);

extern sigset_t	_psigblocked;

enum { Asyncbuf = 1<<20 };
static unsigned char asyncbuf[Asyncbuf];

__attribute__((export_name("__asyncbuf")))
void*
__asyncbuf(void)
{
	return asyncbuf;
}

__attribute__((export_name("__asyncbufsize")))
int
__asyncbufsize(void)
{
	return Asyncbuf;
}

int
setjmp(jmp_buf j)
{
	return __setjmp(j);
}

void
longjmp(jmp_buf j, int v)
{
	if(v == 0)
		v = 1;	/* ansi: "longjmp(0) => longjmp(1)" */
	__longjmp(j, v);
}

int
sigsetjmp(sigjmp_buf j, int savemask)
{
	j[0] = savemask;
	j[1] = (uintptr_t)&_psigblocked;
	return __setjmp(j+2);
}
