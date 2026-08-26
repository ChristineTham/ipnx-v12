/* crt9: the Plan 9-main runtime — void main(), exits() its own way out. */
#include "lib9.h"
extern void main(int, char**);
extern int _sys(int, int, int, int, int, int);
#define SYSARGS 200

__attribute__((export_name("_start")))
void _start(void){
	static char buf[4096];
	static char *argv[64];
	int n, argc;
	char *p;

	n = _sys(SYSARGS, (int)buf, sizeof buf, 0, 0, 0);
	argc = 0;
	for(p = buf; p < buf+n && argc < 63; p += strlen(p)+1)
		argv[argc++] = p;
	argv[argc] = nil;
	main(argc, argv);
	exits("main");
}
