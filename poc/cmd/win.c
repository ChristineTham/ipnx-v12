/* win: run a command in a fresh window — mint one from #w, bind it over
 * /dev in a namespace copy, wire cons to fds 0,1,2, exec. rio's spawn,
 * as a forty-line command. */
#include "lib9.h"

static char **cmdav;
char *smprint(char*);

static void
child(void *v)
{
	char buf[16], path[24];
	int fd, n;

	USED(v);
	fd = open("#w/clone", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	if(n <= 0){
		fprint(2, "win: can't mint a window: %r\n");
		exits("clone");
	}
	buf[n] = 0;
	strcpy(path, "#w/");
	strcpy(path+3, buf);
	bind(path, "/dev", MREPL);
	fd = open("/dev/cons", ORDWR);
	dup(fd, 0);
	dup(fd, 1);
	dup(fd, 2);
	exec(cmdav[0][0] == '/' ? cmdav[0] : smprint(cmdav[0]), cmdav);
	fprint(2, "win: exec: %r\n");
	exits("exec");
}

char *
smprint(char *cmd)
{
	static char path[64];

	strcpy(path, "/bin/");
	strcpy(path+5, cmd);
	return path;
}

int
main(int argc, char *argv[])
{
	char buf[128];
	int pid;

	if(argc < 2){
		fprint(2, "usage: win cmd [args]\n");
		exits("usage");
	}
	cmdav = argv+1;
	pid = procrfork(RFFDG|RFNAMEG, child, nil);
	if(pid < 0){
		fprint(2, "win: %r\n");
		exits("fork");
	}
	await(buf, sizeof buf);
	return 0;
}
