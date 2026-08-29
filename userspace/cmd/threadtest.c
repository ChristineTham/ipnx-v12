/* threadtest: the wasm libthread — coroutines over tsave/tjump contexts,
 * channels with rendezvous and buffering, alt, and the load-bearing claim:
 * a blocking read parks the THREAD while the process keeps scheduling. */
#include <u.h>
#include <libc.h>
#include <thread.h>

static int npass, nfail;
static void
ok(int cond, char *what)
{
	if(cond){ npass++; print("PASS %s\n", what); }
	else    { nfail++; print("FAIL %s\n", what); }
}

static Channel *c1, *c2, *pings;
static int pipefds[2];

static void
pinger(void *v)
{
	int i;

	USED(v);
	for(i = 0; i < 3; i++)
		sendul(pings, i);
	threadexits(nil);
}

static void
adder(void *v)
{
	Channel *c = v;
	ulong n;

	n = recvul(c);
	sendul(c2, n + 100);
	threadexits(nil);
}

static void
reader(void *v)
{
	char buf[32];
	long n;

	USED(v);
	n = read(pipefds[0], buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	ok(n == 5 && strcmp(buf, "hello") == 0,
	   "a blocked read parked the thread, not the process");
	sendul(c2, 999);
	threadexits(nil);
}

void
threadmain(int argc, char *argv[])
{
	ulong v;
	int i, inorder;
	Alt a[3];

	USED(argc); USED(argv);
	pings = chancreate(sizeof(ulong), 0);
	c1 = chancreate(sizeof(ulong), 0);
	c2 = chancreate(sizeof(ulong), 4);

	threadcreate(pinger, nil, 8192);
	inorder = 1;
	for(i = 0; i < 3; i++)
		if(recvul(pings) != i)
			inorder = 0;
	ok(inorder, "unbuffered channel: three rendezvous, in order");

	threadcreate(adder, c1, 8192);
	sendul(c1, 42);
	ok(recvul(c2) == 142, "send met a blocked receiver; the reply came back");

	pipe(pipefds);
	proccreate(reader, nil, 8192);
	yield();
	write(pipefds[1], "hello", 5);
	ok(recvul(c2) == 999, "the reader thread finished after the write");

	sendul(c2, 7);
	a[0].c = c1; a[0].v = &v; a[0].op = CHANRCV;
	a[1].c = c2; a[1].v = &v; a[1].op = CHANRCV;
	a[2].op = CHANEND;
	i = alt(a);
	ok(i == 1 && v == 7, "alt picked the one ready channel");

	if(nfail == 0)
		print("threadtest: all %d passed\n", npass);
	threadexitsall(nfail ? "fail" : nil);
}
