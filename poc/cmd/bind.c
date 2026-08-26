/* bind is an ordinary command, as on Plan 9 — it works because children
 * share the parent's namespace unless rfork(RFNAMEG) says otherwise, so a
 * bind made here lands in the invoking shell's namespace.
 * Flags: -a after, -b before, -c create-permitted (combine as -ac). */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	int flag = MREPL;
	char *p;

	if(argc == 4 && argv[1][0] == '-'){
		for(p = argv[1]+1; *p; p++)
			switch(*p){
			case 'a': flag |= MAFTER; break;
			case 'b': flag |= MBEFORE; break;
			case 'c': flag |= MCREATE; break;
			default:
				fprint(2, "usage: bind [-abc] new old\n");
				exits("usage");
			}
		argv++;
		argc--;
	}
	if(argc != 3){
		fprint(2, "usage: bind [-abc] new old\n");
		exits("usage");
	}
	if(bind(argv[1], argv[2], flag) < 0){
		fprint(2, "bind: %r\n");
		exits("bind");
	}
	return 0;
}
