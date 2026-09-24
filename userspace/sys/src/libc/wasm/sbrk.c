/*
 * `brk` and `sbrk` for the wasm32 architecture.
 *
 * Plan 9's are portable (`libc/9sys/sbrk.c`) because every architecture it
 * runs on reaches the same syscall: `brk_` asks the kernel to move the end of
 * the data segment, and the kernel is the only thing that can.
 *
 * This machine has no data segment and no such call. A module's memory is
 * grown by an instruction — `memory.grow` — which the process executes for
 * itself, and asking a kernel for it would be asking the kernel to do what the
 * machine already does. So the syscall is replaced by the instruction and
 * nothing else changes: the same two functions, the same rounding, the same
 * `(void*)-1` on failure.
 *
 * `bloc` starts where the embedding said the heap begins (`main9.c`), rather
 * than at `end[]`, because the embedding wrote the argument block into this
 * memory before anything of the process ran.
 */
#include <u.h>
#include <libc.h>

enum
{
	Round	= 7,
	Pagesz	= 64*1024,	/* a wasm page, fixed by the format */
};

static char *bloc;

void
_sbrkinit(void *heap)
{
	bloc = heap;
}

/* the address just past the last byte this module's memory holds */
static uintptr
memend(void)
{
	return (uintptr)__builtin_wasm_memory_size(0) * Pagesz;
}

static int
grow(uintptr want)
{
	uintptr have;
	long pages;

	have = memend();
	if(want <= have)
		return 0;
	pages = (want - have + Pagesz - 1) / Pagesz;
	if(__builtin_wasm_memory_grow(0, pages) == (uintptr)-1)
		return -1;
	return 0;
}

int
brk(void *p)
{
	uintptr bl;

	bl = ((uintptr)p + Round) & ~Round;
	if(grow(bl) < 0)
		return -1;
	bloc = (char*)bl;
	return 0;
}

void*
sbrk(ulong n)
{
	uintptr bl;

	bl = ((uintptr)bloc + Round) & ~Round;
	if(grow(bl + n) < 0)
		return (void*)-1;
	bloc = (char*)bl + n;
	return (void*)bl;
}
