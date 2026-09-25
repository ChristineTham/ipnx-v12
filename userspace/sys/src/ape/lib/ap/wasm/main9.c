/*
 * `_main` for APE on the wasm32 architecture — the counterpart of
 * `ape/lib/ap/386/main9.s`, as `libc/wasm/main9.c` is of `libc/386/main9.s`:
 * `_envsetup()`, then `main`, then `exit` with what it answered. The
 * machine calls `_start` with what `libc/wasm/main9.c` says it does — the
 * count, the pointers, where the heap begins, and the `Tos`.
 */
#include "../plan9/lib.h"
#include "../plan9/sys9.h"
#include <stdlib.h>

typedef long long vlong;
typedef unsigned long ulong;
typedef unsigned long long uvlong;

#include "/sys/include/tos.h"

/*
 * 386's `_main` passes `environ` as a third argument, and a `main` of two
 * ignores it. A wasm call must match the callee, and clang names a two-
 * argument `main` `__main_argc_argv`, so it is called as it is defined:
 * with two. No program in the tree defines a `main` of three.
 */
extern	int	main(int, char**);
extern	void	_envsetup(void);
extern	char	**environ;
void	_sbrkinit(void*);

/*
 * `main9p.s` declares these — `GLOBL _tos(SB), $4`, and `_privates` in
 * `_mainp`'s frame — and libap's `profile.c` reads `_tos`.
 */
#define NPRIVATES 16
Tos	*_tos;
void	**_privates;
int	_nprivates;

__attribute__((export_name("_start")))
void
_start(int argc, char *argv[], void *heap, Tos *tos)
{
	void *privates[NPRIVATES];

	_tos = tos;
	_privates = privates;
	_nprivates = NPRIVATES;
	_sbrkinit(heap);
	_envsetup();
	exit(main(argc, argv));
}

/*
 * Where a note is taken — `libc/wasm/main9.c`'s `__notestart`. The note is
 * kept for `_notetramp`, which 386's is given again by the kernel after
 * `NSAVE` (`notetramp.c`).
 */
char	*_notemsg;

__attribute__((export_name("__notestart")))
void
__notestart(int (*f)(void*, char*), void *ureg, char *msg)
{
	_notemsg = msg;
	(*f)(ureg, msg);
	__builtin_trap();
}
