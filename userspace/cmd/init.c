/*
 * `init` — build the namespace, then start the shell.
 *
 * `plan9/sys/src/cmd/init.c`, cut to what this system has. What is left of
 * its `main` (`init.c:23`) is the same sequence:
 *
 *	cpu = readenv("#e/cputype");  setenv("#e/objtype", cpu);
 *	user = readenv("#c/user");    systemname = readenv("#c/sysname");
 *	newns(user, 0);
 *	for(;;){ print("\ninit: starting /bin/rc\n"); fexec(rcexec); manual = 1;
 *		cmd = 0; sleep(1000); }
 *
 * WHAT IS NOT HERE: `-c` and the cpu server's startup (`cpustart`, which
 * runs `/rc/bin/cpurc`; there is one service, a terminal, and no cpurc), the
 * priority write to `#p/n/ctl`, `$timezone`, the password prompt, and
 * `closefds` (this process's three are the console and are meant to be
 * inherited). `-m`, `-t` and a command are here: `init [-mt] [cmd]`, the
 * command given by `boot` from `$init` (`boot.c:208`).
 */
#include <u.h>
#include <libc.h>

char	*service = "terminal";
char	*cmd;
char	*cpu;
char	*user;
char	*systemname;
int	manual;
static int gotnote;

/* `pinhead` (`init.c:119`): a note to init is reported and survived — it
 * is the shell's group the interrupt is for, and init is not in it. */
void
pinhead(void*, char *msg)
{
	gotnote = 1;
	fprint(2, "init got note '%s'\n", msg);
	noted(NCONT);
}

/* `readenv` (`init.c:190`) — a file whose length may be 0 and still have
 * contents, which is true of every file `#c` serves. */
static char*
readenv(char *name)
{
	char *val;
	int f, len;

	f = open(name, OREAD);
	if(f < 0){
		print("init: can't open %s: %r\n", name);
		return "*unknown*";
	}
	len = 64;
	val = malloc(len+1);
	if(val == nil){
		close(f);
		return "*unknown*";
	}
	len = read(f, val, len);
	close(f);
	if(len < 0)
		len = 0;
	while(len > 0 && (val[len-1] == '\n' || val[len-1] == ' '))
		len--;
	val[len] = '\0';
	return val;
}

static void
setenv(char *name, char *val)
{
	int f;

	f = create(name, OWRITE, 0666);
	if(f < 0){
		print("init: can't create %s: %r\n", name);
		return;
	}
	write(f, val, strlen(val));
	close(f);
}

/*
 * `rcexec` (`init.c:171`). Plan 9's terminal case is
 *
 *	rc -c ". /rc/bin/termrc; home=/usr/$user; cd; . lib/profile"
 *
 * — the system's startup, then the user's — and its `manual` case is a bare
 * `rc`. Both are here for the reason Plan 9 has both: the FIRST rc runs the
 * startup and exits, and the one after it is the shell you type at. The two
 * startup scripts are the profiles' `start.rc`s (docs/packages.md), and
 * `$home` is already set, by `newns`.
 */
static void
rcexec(void *v)
{
	USED(v);
	if(cmd)
		exec("/bin/rc", (char*[]){ "rc", "-c", cmd, nil });
	else if(manual)
		exec("/bin/rc", (char*[]){ "rc", nil });
	else
		exec("/bin/rc", (char*[]){ "rc", "-c", ". /profile/start.rc; cd; . /home/profile/start.rc", nil });
	print("init: can't exec /bin/rc: %r\n");
}

/*
 * The end of the session: the user's `stop.rc`, at logout, and then the
 * system's, at shutdown (docs/packages.md). Plan 9 has no counterpart — its
 * terminal is switched off and its user never logs out.
 */
static void
stopexec(void *v)
{
	USED(v);
	exec("/bin/rc", (char*[]){ "rc", "-c", ". /home/profile/stop.rc; . /profile/stop.rc", nil });
	print("init: can't exec /bin/rc: %r\n");
}

static void
stop(void)
{
	Waitmsg *w;
	int pid;

	pid = procrfork(stopexec, nil, 0, RFFDG|RFREND|RFNOTEG);
	if(pid < 0)
		return;
	while((w = wait()) != nil){
		if(w->pid == pid){
			free(w);
			return;
		}
		free(w);
	}
}

void
main(int argc, char *argv[])
{
	Waitmsg *w;
	int pid, bare;

	/* `init.c:34` — without `-c`, whose cpu service is not here */
	ARGBEGIN{
	case 'm':
		manual = 1;
		break;
	case 't':
		service = "terminal";
		break;
	}ARGEND
	cmd = *argv;

	/* `init.c:56` — the name of the machine, and therefore of the
	 * directory its binaries are in. `/profile/namespace` reads it as
	 * `$objtype`, and the machine set `cputype` (`pc/main.c:252`). */
	cpu = readenv("#e/cputype");
	setenv("#e/objtype", cpu);
	user = readenv("#c/user");
	systemname = readenv("#c/sysname");
	setenv("#e/service", service);
	setenv("#e/user", user);
	setenv("#e/sysname", systemname);

	/* `newns(user, 0)` — the namespace this instance is configured to
	 * have, from `/profile/namespace`. */
	if(newns(user, 0) < 0)
		print("init: can't build namespace: %r\n");

	/*
	 * Plan 9 goes round forever (`init.c:66`): the startup rc — or the
	 * one running the command init was given — exits, `manual` becomes 1
	 * and `cmd` 0, and every rc after it is the bare interactive one,
	 * because a terminal does not end, so a shell that exited is a shell
	 * that must be started again.
	 *
	 * **Input CAN end here**, and that is the one difference. A host
	 * terminal closes, `#c`'s `consread` answers that as the `^D` its
	 * user would have typed, and every shell after it reads the same
	 * nothing — so a bare shell's exit is the end of the session rather
	 * than a reason to start another. Plan 9's terminal does not end, so
	 * its loop does not need the test.
	 *
	 * `sleep(1000)` is Plan 9's own last line of the loop and is here for
	 * the reason it is there: a shell that dies at once must not be
	 * restarted at once.
	 */
	for(;;){
		bare = cmd == nil && manual;
		/* `fexec` (`init.c:127`): the child is put in a note group of its
		 * own — *"rfork(RFNOTEG)"* — so an interrupt reaches the shell and
		 * what it runs, and not init. */
		pid = procrfork(rcexec, nil, 0, RFFDG|RFREND|RFNOTEG);
		if(pid < 0){
			print("init: can't start rc: %r\n");
			exits("rc");
		}
		notify(pinhead);
	casedefault:
		gotnote = 0;
		w = wait();
		if(w == nil){
			if(gotnote)
				goto casedefault;
			print("init: wait: %r\n");
			exits(nil);
		}
		if(w->msg[0])
			print("init: rc exit status: %s\n", w->msg);
		free(w);
		if(bare){
			stop();
			exits(nil);
		}
		manual = 1;
		cmd = nil;
		sleep(1000);
	}
}
