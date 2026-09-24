/*
 * The profiling half of `386/main9p.s`: `_savearg` and `_callpc`, which
 * `port/profile.c` calls to find a function's argument and its caller.
 * A wasm function cannot see its caller's pc — the machine's call stack is
 * not in memory — so `_callpc` answers 0, as `port/getcallerpc.c` does for
 * `getcallerpc`. `_mainp`, the profiled entry, is not here: this machine's
 * entry is `_start` (`main9.c`), and nothing links for profiling.
 */
#include <u.h>
#include <libc.h>

long
_savearg(void)
{
	return 0;
}

long
_callpc(void**)
{
	return 0;
}
