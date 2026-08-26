#include "lib9.h"

static void
lsdir(char *path)
{
	uchar buf[4096], *p;
	char name[128];
	int fd, n, sz;

	fd = open(path, OREAD);
	if(fd < 0){
		fprint(2, "ls: can't open %s: %r\n", path);
		exits("open");
	}
	while((n = read(fd, buf, sizeof buf)) > 0)
		for(p = buf; p < buf+n; p += sz){
			sz = (p[0] | p[1]<<8) + 2;	/* size[2] excludes itself */
			print("%s\n", statname(p, name, sizeof name));
		}
	close(fd);
}

int
main(int argc, char *argv[])
{
	uchar edir[512];
	int i;

	if(argc == 1){
		lsdir(".");
		return 0;
	}
	for(i = 1; i < argc; i++){
		if(stat(argv[i], edir, sizeof edir) < 0){
			fprint(2, "ls: %s: %r\n", argv[i]);
			exits("stat");
		}
		if(statmode(edir) & DMDIR)
			lsdir(argv[i]);
		else
			print("%s\n", argv[i]);
	}
	return 0;
}
