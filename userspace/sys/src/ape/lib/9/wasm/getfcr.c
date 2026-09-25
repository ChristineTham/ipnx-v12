/*
 * The floating-point registers — `ape/lib/9/386/getfcr.s`, as
 * `libc/wasm/getfcr.c` says of wasm: round to nearest, no traps, no sticky
 * flags.
 */
#include "../libc.h"
#include <float.h>

unsigned long
getfcr(void)
{
	return FPRNR|FPPDBL;
}

void
setfcr(unsigned long fcr)
{
}

unsigned long
getfsr(void)
{
	return 0;
}

void
setfsr(unsigned long fsr)
{
}
