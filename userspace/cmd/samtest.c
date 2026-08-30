/* samtest: the whole editor stack, headless — sam forks samterm in a
 * window namespace; samterm initdraws, libframe renders the command
 * window; we type through wctl and read the reply's glyphs out of the
 * raster. The round trip crosses: the mesg protocol, libthread's
 * scheduler, async reads, the font cache, and strings-with-background. */
#include "lib9.h"

static int npass, nfail;
static void
ok(int cond, char *what)
{
	if(cond){ npass++; print("PASS %s\n", what); }
	else    { nfail++; print("FAIL %s\n", what); }
}

static char winpath[32];

static void
samchild(void *v)
{
	char *av[] = { "sam9", nil };
	int fd;

	USED(v);
	bind(winpath, "/dev", MREPL);
	fd = open("/dev/cons", ORDWR);
	dup(fd, 0);
	dup(fd, 1);
	dup(fd, 2);
	exec("/bin/sam9", av);   /* the heritage raster stack, by its stepped-back name */
	exits("exec");
}

static int rgbfd;
static int
strip(int wantdark, int wantpale)
{
	/* scan the top 90 rows of the 400-wide window */
	uchar px[4];
	int x, y, dark, pale;

	dark = pale = 0;
	for(y = 0; y < 90; y++)
		for(x = 0; x < 400; x += 2){
			if(pread(rgbfd, px, 4, (vlong)(y*400 + x)*4) != 4)
				return 0;
			if(px[0] < 100 && px[1] < 100 && px[2] < 100)
				dark++;
			if(px[0] > 140 && px[0] < 200 && px[1] > 230)
				pale++;
		}
	return (wantdark < 0 || dark >= wantdark) && (wantpale < 0 || pale >= wantpale);
}

int
main(int argc, char *argv[])
{
	char buf[128];
	int fd, n, i, booted, replied;

	USED(argc); USED(argv);
	fd = open("#w/clone", OREAD);
	n = fd >= 0 ? read(fd, buf, 15) : -1;
	buf[n > 0 ? n : 0] = 0;
	strcpy(winpath, "#w/");
	strcpy(winpath+3, buf);

	procrfork(RFFDG|RFNAMEG, samchild, nil);

	strcpy(buf, winpath);
	strcat(buf, "/rgb");
	rgbfd = open(buf, OREAD);

	/* samterm's first paint: the pale command window over white */
	booted = 0;
	for(i = 0; i < 100 && !booted; i++){
		sleep(100);
		booted = strip(-1, 500);
	}
	ok(booted, "sam+samterm booted: the command window painted");

	/* type garbage; sam must answer ?unknown command — glyph ink */
	strcpy(buf, winpath);
	strcat(buf, "/wctl");
	fd = open(buf, OWRITE);
	write(fd, "type zzz\n", 9);
	close(fd);
	replied = 0;
	for(i = 0; i < 100 && !replied; i++){
		sleep(100);
		replied = strip(120, -1);
	}
	ok(replied, "typed through the window; sam replied; libframe drew the glyphs");

	if(nfail == 0)
		print("samtest: all %d passed\n", npass);
	exits(nfail ? "fail" : "");
	return 0;
}
