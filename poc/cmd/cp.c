#include "lib9.h"

int
main(int argc, char *argv[])
{
	char buf[8192];
	int in, out;
	long n;

	if(argc != 3){
		fprint(2, "usage: cp from to\n");
		exits("usage");
	}
	in = open(argv[1], OREAD);
	if(in < 0){
		fprint(2, "cp: can't open %s: %r\n", argv[1]);
		exits("open");
	}
	out = create(argv[2], OWRITE, 0644);
	if(out < 0){
		fprint(2, "cp: can't create %s: %r\n", argv[2]);
		exits("create");
	}
	while((n = read(in, buf, sizeof buf)) > 0)
		if(write(out, buf, n) != n){
			fprint(2, "cp: write error: %r\n");
			exits("write");
		}
	return 0;
}
