/*
 * `_cycles` — `ape/lib/ap/386/cycles.s` reads the time stamp counter; this
 * machine has none a program can read, and answers 0, as `libc/wasm/cycles.c`
 * does and `profile.c` expects: *"uvlong cycle counter if present, else 0"*.
 */
void
_cycles(unsigned long long *x)
{
	*x = 0;
}
