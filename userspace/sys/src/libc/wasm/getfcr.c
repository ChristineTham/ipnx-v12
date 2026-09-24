/*
 * The floating-point control and status registers — `386/getfcr.s` reads and
 * writes the 387's. wasm has neither: it rounds to nearest, never traps, and
 * keeps no sticky flags. So the control register reads as that, with no trap
 * enabled — `FPRNR`, and none of `FPINEX` … `FPINVAL` — a write asking for
 * a trap cannot be honoured and is not, and the status register reads as
 * nothing having happened.
 */
#include <u.h>
#include <libc.h>

ulong
getfcr(void)
{
	return FPRNR|FPPDBL;
}

void
setfcr(ulong)
{
}

ulong
getfsr(void)
{
	return 0;
}

void
setfsr(ulong)
{
}
