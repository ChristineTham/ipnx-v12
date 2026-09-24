/*
 * The wasm32 machine's half of libthread, as `386.c` is the 386's: a new
 * thread's stack, and the function it starts in.
 *
 * On the 386 `_threadinitstack` pushes f and arg onto the new stack and
 * writes the launcher's pc and the stack pointer into `t->sched`, so the
 * scheduler's `longjmp(t->sched, 1)` lands in the launcher on the new
 * stack. Here the same two words are written, and the machine's `longjmp`
 * does the same with them: finding a pc where its own `setjmp` leaves 0, it
 * calls that function on that stack (libc/wasm/setjmp.c; RESEARCH §16.12).
 * The function is called with the stack pointer, where f and arg are.
 */
#include <u.h>
#include <libc.h>
#include <thread.h>
#include "threadimpl.h"

static void
launcherwasm(void *v)
{
	ulong *a;

	a = v;
	(*(void(*)(void*))a[0])((void*)a[1]);
	threadexits(nil);
}

void
_threadinitstack(Thread *t, void (*f)(void*), void *arg)
{
	ulong *tos;

	tos = (ulong*)&t->stk[t->stksize&~7];
	*--tos = (ulong)arg;
	*--tos = (ulong)f;
	t->sched[JMPBUFPC] = (ulong)launcherwasm+JMPBUFDPC;
	t->sched[JMPBUFSP] = (ulong)tos;
}
