/*
 * `argv0` — 386 keeps this in `argv0.s`, which is four words of data. It is
 * here for the same reason: the definition has to live somewhere in libc, and
 * in Plan 9 that somewhere is the architecture directory.
 */
#include <u.h>
#include <libc.h>

char *argv0;

/*
 * `getcallerpc` is `386/getcallerpc.s`, one instruction reading the saved
 * return address out of the frame. wasm has no addressable return address —
 * the call stack is the engine's and cannot be read — so this answers 0.
 * Only the pool allocator's debugging uses it (`port/pool.c`), where it is a
 * label in a panic message.
 */
uintptr
getcallerpc(void *x)
{
	USED(x);
	return 0;
}
