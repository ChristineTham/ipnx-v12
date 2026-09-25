/*
 * PROPOSED (docs/proposals.md) — the wasm32 architecture's Ureg.
 *
 * Every Plan 9 architecture has one (`386/include/ureg.h`): the registers a
 * trap saves, which a note handler is given and a debugger reads. This
 * machine has no register set a program can see — the engine's value stack
 * is out of its reach — and the machine hands a note handler nil
 * (`libc/wasm/main9.c`, `__notestart`). What a program CAN name of its own
 * state is the two words its jmp_buf holds (`u.h`): a function to run, as
 * an index into the function table, and the stack pointer, the
 * `__stack_pointer` global.
 */
struct Ureg
{
	ulong	pc;
	ulong	sp;
};
