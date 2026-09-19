/*
 * Locks for the wasm32 architecture.
 *
 * Plan 9's `libc/port/lock.c` takes a lock with `ainc` — an atomic increment
 * the architecture supplies (`386/atom.s`) — and waits in the kernel with
 * `semacquire` when it loses the race. Both halves are the machine's: the
 * instruction is architectural, and the wait is a syscall.
 *
 * A process on this machine is one instance with one thread of control, so
 * there is no race to lose. The counter is kept and checked all the same, so
 * that a lock taken twice is a fault rather than a silent success — which is
 * what a deadlock would have been on Plan 9.
 */
#include <u.h>
#include <libc.h>

void
lock(Lock *l)
{
	if(l->key++ != 0){
		l->key--;
		sysfatal("lock: held, and this machine has one thread");
	}
}

void
unlock(Lock *l)
{
	l->key = 0;
}

int
canlock(Lock *l)
{
	if(l->key)
		return 0;
	l->key = 1;
	return 1;
}
