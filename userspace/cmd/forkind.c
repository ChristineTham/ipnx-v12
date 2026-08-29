/* forkind: bare-fork shapes that mirror rc's Xpipe/Xsubshell — the child
 * CONTINUES running C (no exec), the parent rewinds and reaps. */
#include "lib9.h"

static int phase;

static void
doforkexec(void)
{
	char *av[] = { "echo", "child-ran", nil };
	int pid;
	char buf[64];

	pid = rfork(RFFDG|RFREND|RFPROC);
	if(pid < 0)
		exits("rfork failed");
	if(pid == 0){
		exec("/bin/echo", av);
		exits("exec failed");
	}
	await(buf, sizeof buf);
	print("parent reaped %d ok phase %d\n", pid, phase);
}

static void
dopipefork(void)
{
	int pfd[2], pid, n;
	char buf[64];

	if(pipe(pfd) < 0)
		exits("pipe failed");
	pid = rfork(RFFDG|RFREND|RFPROC);
	if(pid < 0)
		exits("rfork failed");
	if(pid == 0){
		close(pfd[0]);
		write(pfd[1], "through the pipe", 16);
		close(pfd[1]);
		exits("");
	}
	close(pfd[1]);
	n = read(pfd[0], buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(pfd[0]);
	await(buf + 32, 31);
	print("pipe said '%s' phase %d\n", buf, phase);
}

void (*vmop)(void);

int
main(int argc, char *argv[])
{
	USED(argc); USED(argv);
	phase = 1;
	doforkexec();
	phase = 2;
	vmop = doforkexec;
	(*vmop)();
	phase = 3;
	dopipefork();		/* child continues in C, no exec */
	phase = 4;
	vmop = dopipefork;
	(*vmop)();
	print("forkind done\n");
	exits("");
	return 0;
}
