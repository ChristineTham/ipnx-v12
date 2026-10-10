/*
 * ipnx: modified 2026-09-28 by Christine Tham, for ipnx-v12
 * (https://github.com/ChristineTham/ipnx-v12): the case for Twasm below,
 * this machine, whose header genarch writes (mkfile: $objtype.h).
 */
#ifndef _ARCH_H
#define _ARCH_H
#ifdef T386
#include "386.h"
#elif Tmips
#include "mips.h"
#elif Tspim
#include "spim.h"
#elif Tpower
#include "mips.h"
#elif Talpha
#include "alpha.h"
#elif Tarm
#include "arm.h"
#elif Tarm64
#include "arm64.h"
#elif Tamd64
#include "amd64.h"
#elif Triscv
#include "riscv.h"
#elif Triscv64
#include "riscv64.h"
#elif Tsparc
#include "sparc.h"
#elif Tpower64
#include "power64.h"
#elif Twasm	/* ipnx: this machine's, which genarch writes (mkfile: $objtype.h) */
#include "wasm.h"
#else
	I do not know about your architecture.
	Update switch in arch.h with new architecture.
#endif	/* T386 */
#endif /* _ARCH_H */
