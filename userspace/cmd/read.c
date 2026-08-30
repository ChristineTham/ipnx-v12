/* read: one read(2) from standard input, written to standard output —
 * one line from a line-per-read device (canvas events, cons). Exits
 * "eof" on end of file, so rc can loop: while(l=`{read}) ... */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char buf[8192];
	long n;

	USED(argc); USED(argv);
	n = read(0, buf, sizeof buf);
	if(n > 0)
		write(1, buf, n);
	exits(n > 0 ? 0 : "eof");
}
