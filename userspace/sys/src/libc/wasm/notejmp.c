/*
 * `notejmp` — leave a note handler for a `setjmp` point instead of for where
 * the note found the process. `386/notejmp.c` does it by editing the Ureg:
 * the saved pc and sp become the jmp_buf's, and `noted(NCONT)` restores them.
 *
 * This machine has no Ureg to edit: the handler is entered by the machine
 * from inside the call the note interrupted (`hosts/ipnx/src/machine.rs`,
 * `handler`), and the process's own frames lie beneath the machine's. A
 * `longjmp` from the handler cannot cross those. So the jump is recorded,
 * `noted(NCONT)` leaves the handler as it would, the interrupted call
 * returns to its stub in `sys.c` — the process's own frame again — and the
 * stub makes the jump (`_notejmped`).
 *
 * One difference follows, and it is this machine's: a note taken at a clock
 * interrupt rather than at a call jumps at the process's next call.
 */
#include <u.h>
#include <libc.h>

static long	*jmpto;
static int	jmpret;

void
notejmp(void*, jmp_buf j, int ret)
{
	jmpto = j;
	jmpret = ret;
	noted(NCONT);
}

void
_notejmped(void)
{
	long *j;

	if(jmpto == nil)
		return;
	j = jmpto;
	jmpto = nil;
	longjmp(j, jmpret);
}
