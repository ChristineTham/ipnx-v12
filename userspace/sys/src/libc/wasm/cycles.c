/*
 * `cycles` — `386/cycles.s` reads the time stamp counter. `libc.h:353` says
 * what a machine without one answers: *"64-bit value of the cycle counter if
 * there is one, 0 if there isn't"*. This machine has none a program can read.
 */
#include <u.h>
#include <libc.h>

void
cycles(uvlong *x)
{
	*x = 0;
}
