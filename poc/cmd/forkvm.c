/* forkvm: rc's Xpipe, structurally verbatim — refcounted code array,
 * arg slots after the op, a jump-target slot, start() pushing a thread —
 * distilled to the smallest module that runs the same rewind. */
#include "lib9.h"

typedef union code code;
union code { void (*f)(void); int i; char *s; };
struct thread { code *code; int pc; struct thread *ret; };
struct thread *runq;

static code *
codecopy(code *cp)
{
	cp[0].i++;
	return cp;
}

static void
start(code *c, int pc)
{
	struct thread *p = (struct thread *)malloc(sizeof(struct thread));

	p->code = codecopy(c);
	p->pc = pc;
	p->ret = runq;
	runq = p;
}

static void
Xpipe(void)
{
	struct thread *p = runq;
	int pc = p->pc, forkid;
	int lfd = p->code[pc++].i;
	int rfd = p->code[pc++].i;
	int pfd[2];

	if(pipe(pfd) < 0)
		exits("pipe");
	switch(forkid = fork()){
	case -1:
		exits("fork");
	case 0:
		start(p->code, pc+2);
		close(pfd[0]);
		print("child side, lfd %d\n", lfd);
		close(pfd[1]);
		exits("");
	default:
		start(p->code, p->code[pc].i);
		close(pfd[1]);
		USED(rfd);
		p->pc = p->code[pc+1].i;
		break;
	}
}

static void
Xhello(void)
{
	print("right side ran, pc advanced correctly\n");
}

static void
Xdone(void)
{
	char buf[64];

	await(buf, sizeof buf);
	print("forkvm done\n");
	exits("");
}

static void
Xret(void)
{
	runq = runq->ret;
}

void
main(int argc, char *argv[])
{
	static code prog[16];
	int i;

	USED(argc); USED(argv);
	i = 0;
	prog[i++].i = 1;		/* [0] refcount, rc's layout */
	prog[i++].f = Xpipe;		/* [1] */
	prog[i++].i = 1;		/* [2] lfd */
	prog[i++].i = 0;		/* [3] rfd */
	prog[i++].i = 8;		/* [4] right side pc */
	prog[i++].i = 10;		/* [5] end pc */
	prog[i++].f = Xret;		/* [6] left: pop thread */
	prog[i++].f = Xdone;		/* [7] (left never gets here) */
	prog[i++].f = Xhello;		/* [8] right side */
	prog[i++].f = Xret;		/* [9] */
	prog[i++].f = Xdone;		/* [10] end */
	start(prog, 1);
	for(;;)
		(*runq->code[runq->pc++].f)();
}
