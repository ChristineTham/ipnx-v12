/*
 * `_tas` — test and set, as `386/tas.s` does it with XCHGL: store a nonzero
 * word and answer what was there. `port/lock.c` is built on it.
 *
 * The builtin is the machine's exchange. Without wasm threads it compiles to
 * a load and a store, which is all atomicity asks of memory that one process
 * owns; with them it is `i32.atomic.rmw.xchg`.
 */
#include <u.h>
#include <libc.h>

int
_tas(int *l)
{
	return __atomic_exchange_n(l, 0xdeadead, __ATOMIC_SEQ_CST);
}
