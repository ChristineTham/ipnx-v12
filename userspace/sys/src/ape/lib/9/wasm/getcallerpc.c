/*
 * `getcallerpc` — `ape/lib/9/386/getcallerpc.s`. A wasm function cannot see
 * its caller's pc, and answers 0, as `libc/port/getcallerpc.c` does.
 */
#include "../libc.h"

uintptr_t
getcallerpc(void *x)
{
	return 0;
}
