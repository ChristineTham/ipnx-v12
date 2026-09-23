/*
 * `procrfork` for the wasm32 architecture — making a process on a machine
 * whose stack cannot be duplicated.
 *
 * `rfork(RFPROC)` returns TWICE: once into the parent with the child's pid,
 * once into the child with 0. Every other thing it does — copying the file
 * descriptor table, the namespace, the environment group — is table work in
 * the kernel, and this machine does all of it. What it cannot do is the
 * double return. A wasm call returns into the engine's own stack, and nothing
 * outside the engine can duplicate one: the stack-switching proposal excludes
 * process duplication by name, and JSPI suspends a stack rather than copying
 * it (RESEARCH §5.2, measured).
 *
 * So the child is told WHERE TO START instead of resuming a copy of the
 * parent — which is Plan 9's own shape for the same thing:
 *
 *	int procrfork(void (*f)(void*), void *arg, uint stacksize, int rforkflag)
 *		— libthread/create.c:103
 *
 * Three differences from libthread's. It answers the child's PID where
 * libthread answers a thread id, because there are no threads here.
 * `stacksize` is accepted and ignored: the child's stack is where the
 * parent's was. And **the memory is a copy, not shared** — libthread adds
 * `RFMEM`, and this machine cannot: one wasm memory cannot belong to two
 * instances, and one instance cannot run on two stacks. So the child is a
 * new instance of the same image with the parent's memory copied into it,
 * starting at `f(arg)` from the parent's stack pointer — `arg` may point
 * into the parent's frames, and the copy has them — and the kernel is asked
 * for `RFPROC` and the caller's flags, which is `rfork`'s own copy of the
 * data segment. A caller asking for `RFMEM` is refused.
 *
 * The child is a process of its own from its first instruction: it may
 * sleep before it `exec`s — waiting for a pipe, or for a file server to
 * answer — and the parent goes on meanwhile, as on Plan 9.
 */
#include <u.h>
#include <libc.h>

__attribute__((import_module("sys"), import_name("procrfork")))
extern int __procrfork(void (*)(void*), void*, uint, int);

int
procrfork(void (*f)(void *), void *arg, uint stacksize, int rforkflag)
{
	return __procrfork(f, arg, stacksize, rforkflag);
}

/*
 * Where the child starts. The machine calls this, on this instance, with the
 * process now being the child: `f` is a function pointer, which on this
 * machine is an index into the module's one table, and calling it is the
 * `call_indirect` the compiler emits for any indirect call.
 *
 * It returns only if the child's function does. A child that execs or exits
 * never comes back here — the machine unwinds it — which is exactly what
 * happens on Plan 9, by a different route.
 */
__attribute__((export_name("__childstart")))
void
__childstart(void (*f)(void*), void *arg)
{
	(*f)(arg);
	exits("child returned");
}

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
