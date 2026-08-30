/* kill: post a note (default "kill") to each pid. */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	int i, pid, bad;
	char *note, *p;

	note = "kill";
	i = 1;
	if(argc > 2 && argv[1][0] == '-'){
		note = argv[1] + 1;
		i = 2;
	}
	if(i >= argc){
		fprint(2, "usage: kill [-note] pid ...\n");
		exits("usage");
	}
	bad = 0;
	for(; i < argc; i++){
		pid = 0;
		for(p = argv[i]; *p >= '0' && *p <= '9'; p++)
			pid = pid * 10 + (*p - '0');
		if(pid <= 0 || postnote(PNPROC, pid, note) < 0){
			fprint(2, "kill: %s: %r\n", argv[i]);
			bad = 1;
		}
	}
	exits(bad ? "error" : 0);
}
