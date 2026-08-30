/* sleep: seconds, as sleep(1) always counted them. */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	long s;
	char *p;

	if(argc != 2){
		fprint(2, "usage: sleep seconds\n");
		exits("usage");
	}
	s = 0;
	for(p = argv[1]; *p >= '0' && *p <= '9'; p++)
		s = s * 10 + (*p - '0');
	sleep(s * 1000);
	exits(0);
}
