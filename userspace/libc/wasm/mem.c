/*
 * `memcpy` for the wasm32 architecture.
 *
 * Plan 9 writes this one in assembly for every architecture it runs on
 * (`386/memcpy.s`, `arm/memcpy.s`, …) and keeps no portable version, because
 * the whole point of it is the machine's widest move instruction. `port/` has
 * C for its neighbours — `memmove.c`, `memset.c`, `memcmp.c` — and those are
 * used here unchanged.
 *
 * The machine's move instruction is `memory.copy`, which the compiler emits
 * for a `memcpy` it recognises. This body must therefore be built with
 * `-fno-builtin`, or clang rewrites the loop below into a call to itself —
 * the same finding the toolchain notes record for `strlen` (RESEARCH §9.4).
 */
#include <u.h>
#include <libc.h>

void*
memcpy(void *a, void *b, ulong n)
{
	uchar *s, *d;

	d = a;
	s = b;
	while(n-- > 0)
		*d++ = *s++;
	return a;
}
