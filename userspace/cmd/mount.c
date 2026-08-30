/* mount: attach a posted service (srv(3)) or any 9P fd path onto old.
 * A bare name means /srv/name. -a and -b stack as bind(1)'s do. */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	int fd, flag, i;
	char *src, *old, buf[128];

	flag = MREPL;
	i = 1;
	if(argc > 1 && strcmp(argv[1], "-a") == 0){ flag = MAFTER; i = 2; }
	else if(argc > 1 && strcmp(argv[1], "-b") == 0){ flag = MBEFORE; i = 2; }
	if(argc - i != 2){
		fprint(2, "usage: mount [-a|-b] service old\n");
		exits("usage");
	}
	src = argv[i];
	old = argv[i+1];
	if(src[0] != '/' && src[0] != '#'){
		snprint(buf, sizeof buf, "/srv/%s", src);
		src = buf;
	}
	fd = open(src, ORDWR);
	if(fd < 0){
		fprint(2, "mount: %s: %r\n", src);
		exits("open");
	}
	if(mount(fd, -1, old, flag, "") < 0){
		fprint(2, "mount: %s on %s: %r\n", src, old);
		exits("mount");
	}
	exits(0);
}
