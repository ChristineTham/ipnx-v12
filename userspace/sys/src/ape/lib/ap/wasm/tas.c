/*
 * `tas` — `ape/lib/ap/386/tas.s`, the machine's exchange (`libc/wasm/tas.c`).
 */
int
tas(int *l)
{
	return __atomic_exchange_n(l, 0xdeadead, __ATOMIC_SEQ_CST);
}
