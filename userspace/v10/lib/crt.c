/* libv10 crt: int main(argc, argv), stdio flushed on the way out. */
#include "lib9.h"
extern int main(int, char**);
extern void _v10flush(void);
extern int _sys(int, int, int, int, int, int);
#define SYSARGS 200

__attribute__((export_name("_start")))
void _start(void){
	static char buf[4096];
	static char *argv[64];
	int n, argc, r;
	char *p;

	n = _sys(SYSARGS, (int)buf, sizeof buf, 0, 0, 0);
	argc = 0;
	for(p = buf; p < buf+n && argc < 63; p += strlen(p)+1)
		argv[argc++] = p;
	argv[argc] = nil;
	r = main(argc, argv);
	_v10flush();
	exits(r == 0 ? nil : "error");
}
