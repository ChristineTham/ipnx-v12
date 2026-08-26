#include "lib9.h"

static long lines, words, chars;

static void
count(int fd)
{
	char buf[8192];
	long n, i;
	int inword = 0;

	while((n = read(fd, buf, sizeof buf)) > 0){
		chars += n;
		for(i = 0; i < n; i++){
			if(buf[i] == '\n')
				lines++;
			if(buf[i]==' ' || buf[i]=='\t' || buf[i]=='\n')
				inword = 0;
			else if(!inword){
				inword = 1;
				words++;
			}
		}
	}
}

int
main(int argc, char *argv[])
{
	int i, fd;

	if(argc == 1)
		count(0);
	for(i = 1; i < argc; i++){
		fd = open(argv[i], OREAD);
		if(fd < 0){
			fprint(2, "wc: can't open %s: %r\n", argv[i]);
			exits("open");
		}
		count(fd);
		close(fd);
	}
	print("%d %d %d\n", (int)lines, (int)words, (int)chars);
	return 0;
}
