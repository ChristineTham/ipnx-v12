/* notetest: the note machinery, observed from inside one process.
 * alarm(0) cancels; a live alarm interrupts a blocked read (-1, errstr
 * "interrupted", handler sees "alarm"); a self-note posted through
 * /proc/<pid>/note is delivered at the posting syscall's own return.
 * Exit status is the verdict: empty means every check held. */
#include "lib9.h"

static int got;
static char note[64];

static void
h(void *v, char *s)
{
	USED(v);
	strncpy(note, s, sizeof note - 1);
	got++;
	noted(NCONT);
}

int
main(int argc, char *argv[])
{
	int fd[2];
	char err[128], buf[8];
	long n;

	USED(argc); USED(argv);
	notify(h);

	alarm(100);
	alarm(0);
	sleep(150);
	if(got != 0)
		exits("cancelled alarm fired");

	pipe(fd);
	alarm(50);
	n = read(fd[0], buf, sizeof buf);
	errstr(err, sizeof err);
	if(n != -1)
		exits("read not interrupted");
	if(strstr(err, "interrupted") == nil)
		exits("errstr not interrupted");
	if(got != 1 || strcmp(note, "alarm") != 0)
		exits("no alarm note");

	postnote(PNPROC, getpid(), "hello");
	if(got != 2 || strcmp(note, "hello") != 0)
		exits("self-note not delivered");
	exits("");
	return 0;
}
