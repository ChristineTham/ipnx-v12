/*
 * `brk` and `sbrk` — `ape/lib/ap/plan9/brk.c`, replaced here as
 * `riscv64/brk.c` replaces it there. The call, `_BRK_`, is `memory.grow`
 * on this machine, as `libc/wasm/sbrk.c` says; the rounding and `ENOMEM`
 * are plan9/brk.c's.
 */
#include "../plan9/lib.h"
#include <errno.h>
#include <stdlib.h>
#define _SUSV2_SOURCE	/* plan9/brk.c has it from its mkfile */
#include <inttypes.h>

enum { Pagesz = 64*1024 };	/* a wasm page, fixed by the format */

static char *bloc;

void
_sbrkinit(void *heap)
{
	bloc = heap;
}

static int
_BRK_(void *p)
{
	uintptr_t have, want;
	long pages;

	want = (uintptr_t)p;
	have = (uintptr_t)__builtin_wasm_memory_size(0) * Pagesz;
	if(want <= have)
		return 0;
	pages = (want - have + Pagesz - 1) / Pagesz;
	if(__builtin_wasm_memory_grow(0, pages) == (uintptr_t)-1)
		return -1;
	return 0;
}

int
brk(char *p)
{
	uintptr_t n;

	n = (uintptr_t)p + sizeof(uintptr_t) - 1;
	n &= ~((uintptr_t)sizeof(uintptr_t) - 1);
	if(_BRK_((void*)n) < 0){
		errno = ENOMEM;
		return -1;
	}
	bloc = (char *)n;
	return 0;
}

void *
sbrk(uintptr_t n)
{
	if ((intptr_t)n < 0)
		abort();
	n += sizeof(uintptr_t) - 1;
	n &= ~((uintptr_t)sizeof(uintptr_t) - 1);
	if(_BRK_((void *)(bloc+n)) < 0){
		errno = ENOMEM;
		return (void *)-1;
	}
	bloc += n;
	return (void *)(bloc-n);
}
