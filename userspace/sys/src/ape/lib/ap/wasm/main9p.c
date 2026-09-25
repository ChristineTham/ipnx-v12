/*
 * The profiling half of `ape/lib/ap/386/main9p.s` — `_savearg` and
 * `_callpc`, as `libc/wasm/main9p.c` has them: a wasm function cannot see
 * its caller's pc, so `_callpc` answers 0. `_mainp`, the profiled entry,
 * is not here: this machine's entry is `_start` (`main9.c`).
 */
long
_savearg(void)
{
	return 0;
}

long
_callpc(void **p)
{
	return 0;
}
