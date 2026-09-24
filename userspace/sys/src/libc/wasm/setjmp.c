/*
 * `setjmp` and `longjmp` — the machine-dependent pair, as Plan 9's are:
 * `libc/386/setjmp.s` saves SP and the return pc and `longjmp` puts them
 * back. This machine's call stack is the engine's, where a program cannot
 * reach it, so neither can be saved by the program. The machine does it:
 * every image is transformed after linking by Binaryen's asyncify (mk.sh),
 * which lets a stack be unwound into memory and wound back up, and these
 * two are calls to the machine that do exactly that (Christine, 2026-09-24:
 * asyncify, for `fork` and libthread as well — RESEARCH §16.12).
 *
 *   setjmp(j)      the stack unwinds into `asyncbuf`, the machine keeps a
 *                  copy of it and the stack pointer under j, and winds it
 *                  back: the call answers 0.
 *   longjmp(j, v)  the stack unwinds and is dropped, the copy kept under j
 *                  is wound back instead, and setjmp's call answers v —
 *                  *"ansi: longjmp(0) => longjmp(1)"*, as setjmp.s says.
 *
 * `jmp_buf` holds nothing the machine reads: its address is the key.
 */
#include <u.h>
#include <libc.h>

#define SYS(name) __attribute__((import_module("sys"), import_name(#name)))

SYS(setjmp)	extern int	__setjmp(long*);
SYS(longjmp)	extern void	__longjmp(long*, int);

/*
 * Where a stack unwinds to: asyncify's two words — the next free byte and
 * the end — then the frames. A megabyte of bss costs the image nothing.
 */
enum { Asyncbuf = 1<<20 };
static uchar asyncbuf[Asyncbuf];

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
		v = 1;
	__longjmp(j, v);
}
