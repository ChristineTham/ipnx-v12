#ifndef __UREG_H
#define __UREG_H
#if !defined(_PLAN9_SOURCE)
    This header file is an extension to ANSI/POSIX
#endif

/*
 * PROPOSED (docs/proposals.md) — the wasm32 architecture's registers, as
 * `wasm/include/ureg.h` has them: what a program can name of its own state
 * is a function to run (`pc`, an index into the function table, as a
 * jmp_buf's is) and the stack pointer (`sp`, the `__stack_pointer` global).
 */
struct Ureg
{
	unsigned long	pc;
	unsigned long	sp;
};

#endif
