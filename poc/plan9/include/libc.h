/* shim libc.h: Plan 9's libc surface, served by the PoC's lib9 */
#include "lib9.h"

extern char *argv0;
void sysfatal(char *fmt, ...);
