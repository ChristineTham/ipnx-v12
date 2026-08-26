/* bind is an ordinary command, as on Plan 9 — it works because children
 * share the parent's namespace unless rfork(RFNAMEG) says otherwise, so a
 * bind made here lands in the invoking shell's namespace. */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	if(argc != 3){
		fprint(2, "usage: bind new old\n");
		exits("usage");
	}
	if(bind(argv[1], argv[2], MREPL) < 0){
		fprint(2, "bind: %r\n");
		exits("bind");
	}
	return 0;
}
