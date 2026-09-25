/*
 * `_notetramp` and `siglongjmp` — `ape/lib/ap/386/notetramp.c`.
 *
 * 386's leaves the note with `NSAVE` after pointing the Ureg's pc at
 * `notecont`, so the handler runs outside the note, and `NRSTR` puts the
 * saved state back. This machine has no Ureg to point (the machine hands
 * the handler nil, `main9.c`): the handler is called here, still inside the
 * note, with the note `__notestart` kept, and the note is left with
 * `NCONT` — what `NSAVE` then `NRSTR` come to.
 *
 * `siglongjmp` out of a handler cannot cross the machine's frames beneath
 * it, as `libc/wasm/notejmp.c` says; the jump is recorded, the note left,
 * and the call the note interrupted makes it on its way back (`sys.c`,
 * `_notejmped`). Outside a handler it is 386's: the mask, then `longjmp`.
 */
#include "../plan9/lib.h"
#include "../plan9/sys9.h"
#include <signal.h>
#include <setjmp.h>

extern sigset_t	_psigblocked;
extern char	*_notemsg;

static int	nstack;
static uintptr_t	*jmpto;
static int	jmpret;

void
_notetramp(int sig, void (*hdlr)(int, char*, Ureg*), Ureg *u)
{
	nstack++;
	(*hdlr)(sig, _notemsg, u);
	nstack--;
	_NOTED(0);	/* NCONT */
}

void
siglongjmp(sigjmp_buf j, int ret)
{
	if(j[0])
		_psigblocked = j[1];
	if(nstack == 0)
		longjmp(j+2, ret);
	nstack--;
	jmpto = j+2;
	jmpret = ret;
	_NOTED(0);	/* NCONT */
}

void
_notejmped(void)
{
	uintptr_t *j;

	if(jmpto == 0)
		return;
	j = jmpto;
	jmpto = 0;
	longjmp(j, jmpret);
}
