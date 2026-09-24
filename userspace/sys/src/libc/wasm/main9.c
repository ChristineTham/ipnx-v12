/*
 * `_main` for the wasm32 architecture — the counterpart of
 * `plan9/sys/src/libc/386/main9.s`.
 *
 * There, `sysexec` has already built the new process's stack: the argument
 * count and the argument pointers sit in the frame, and `touser` jumps to
 * `_main`, which reads `inargc-4(FP)`, takes the address of `inargv+0(FP)`,
 * and calls `main(argc, argv)`. If main returns it calls `exits("main")`.
 *
 * This machine has no stack to have built. The embedding instantiates the
 * module, writes the argument block into the module's own memory, and calls
 * the exported entry with three numbers: the count, the address of the
 * pointer array, and the address just past the block — which is where this
 * process's heap begins, because everything below it is spoken for.
 *
 * That third argument is the only thing here 386's version has no counterpart
 * for, and it exists for the same reason the rest of the file differs: a wasm
 * module's memory is not laid out by whoever loaded it, so the loader must say
 * what it used.
 */
#include <u.h>
#include <libc.h>
#include <tos.h>

extern void main(int, char*[]);

/*
 * `_privates` and `_nprivates` — 386's `main9.s` reserves NPRIVATES words in
 * the frame and points `_privates` at them. `privalloc` (`9sys/privalloc.c`)
 * hands them out. Here they are static, because there is no frame to reserve
 * them in.
 */
#define NPRIVATES 16
void	*_privates[NPRIVATES];
int	_nprivates = NPRIVATES;

void	_sbrkinit(void*);

/*
 * `tos` is where `main9.s` finds it in AX: the machine puts the `Tos` at the
 * top of the stack, as Plan 9's kernel does at USTKTOP, and writes the pid
 * into it, as `kexit` does (`pc/trap.c:302`).
 */
__attribute__((export_name("_start")))
void
_start(int argc, char *argv[], void *heap, Tos *tos)
{
	_tos = tos;
	_sbrkinit(heap);
	main(argc, argv);
	exits("main");
}

/*
 * `_tos` — the top-of-stack structure. Plan 9's kernel puts it at the top
 * of every process's stack (`portdat.h`'s `Tos`, `sys/include/tos.h`)
 * holding the process id, the cycle-counter frequency and the kernel's
 * clock, and `main9.s` takes its address out of AX, where `touser` left it.
 * Here the machine puts it at the top of the stack too, and `_start` is
 * given its address. libthread reads the pid from it (`sched.c`).
 */
Tos *_tos;

/*
 * Where a note is taken. The machine calls this, on this instance, when the
 * kernel hands it a note for the process's handler: `f` is what `notify(2)`
 * was given, and `msg` is the note, which the machine has written onto this
 * process's stack below its stack pointer — `notify(Ureg*)`'s own
 * arrangement (pc/trap.c:834-857). `ureg` is nil: there is no register set
 * on this machine for a handler to see.
 *
 * The handler leaves through `noted`, which never returns here. If it
 * returns instead, it has returned into nothing — on Plan 9 into pc 0, a
 * fault — and a fault is what this is.
 */
__attribute__((export_name("__notestart")))
void
__notestart(void (*f)(void*, char*), void *ureg, char *msg)
{
	(*f)(ureg, msg);
	__builtin_trap();
}
