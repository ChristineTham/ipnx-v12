/* forktest: the bare dual-return rfork, over asyncify.
 * Everything the lazy path cannot do: both sides continue interpreting,
 * and the child's memory is a copy, not a share. */
#include "lib9.h"

int g = 1;

int
main(int argc, char *argv[])
{
	char buf[128];
	volatile int l;
	int pid, n, bad;

	USED(argc); USED(argv);
	bad = 0;
	l = 2;
	pid = rfork(RFFDG|RFPROC);
	if(pid < 0){
		fprint(2, "forktest: rfork: %r\n");
		exits("rfork");
	}
	if(pid == 0){
		if(g == 1 && l == 2)
			print("PASS bare fork: child rewound with copied state\n");
		else
			print("FAIL bare fork: child state wrong\n");
		g = 42;
		l = 43;
		exits(nil);
	}
	n = await(buf, sizeof buf);
	if(n > 0 && atoi(buf) == pid)
		print("PASS bare fork: dual return, pid to parent, await reaps\n");
	else { print("FAIL bare fork: await\n"); bad = 1; }
	if(g == 1)
		print("PASS bare fork: memory copied, not shared\n");
	else { print("FAIL bare fork: child write leaked into parent\n"); bad = 1; }
	if(l == 2)
		print("PASS bare fork: parent locals intact after rewind\n");
	else { print("FAIL bare fork: parent locals\n"); bad = 1; }
	return bad;
}
