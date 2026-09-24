/*
 * The atomic operations `386/atom.s` supplies: `ainc` and `adec`, which
 * answer the new value, and the compare-and-swap family, which answer
 * whether the swap was made. 386 does each with LOCK CMPXCHG; these are the
 * compiler's builtins for the same thing (see `tas.c`).
 */
#include <u.h>
#include <libc.h>

long
ainc(long *p)
{
	return __atomic_add_fetch(p, 1, __ATOMIC_SEQ_CST);
}

long
adec(long *p)
{
	return __atomic_sub_fetch(p, 1, __ATOMIC_SEQ_CST);
}

int
cas32(u32int *p, u32int ov, u32int nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int
cas(uint *p, int ov, int nv)
{
	return cas32((u32int*)p, ov, nv);
}

int
casp(void **p, void *ov, void *nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int
casl(ulong *p, ulong ov, ulong nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int
cas64(u64int *p, u64int ov, u64int nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
