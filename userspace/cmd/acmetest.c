/* acmetest: the whole of acme, headless — acme boots in a window
 * namespace (initdraw, initmouse, initkeyboard, its own 9P file server
 * mounted over a pipe, procexec machinery armed), paints its row and
 * column tags, and then a button-2 chord on "New" in a column tag opens
 * a window whose tag ink is read back out of the raster. The round trip
 * crosses: libthread's scheduler, the mouse channel, acme's Edit loop,
 * libframe, and the font cache over the *default* subfont. */
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
acmechild(void *v)
{
	char *av[] = { "acme9", nil };
	int fd;

	USED(v);
	bind(winpath, "/dev", MREPL);
	fd = open("/dev/cons", ORDWR);
	dup(fd, 0);
	dup(fd, 1);
	/* fd 2 stays on the test console: acme's errors surface in the log */
	exec("/bin/acme9", av);
	exits("exec");
}

static int rgbfd;

/* count dark (inked) pixels in rows y0..y1 of the 400-wide window */
static int
darkin(int y0, int y1)
{
	uchar row[400*4];
	int x, y, dark;

	dark = 0;
	for(y = y0; y <= y1; y++){
		if(pread(rgbfd, row, sizeof row, (vlong)y*400*4) != sizeof row)
			return -1;
		for(x = 0; x < 400; x++)
			if(row[x*4] < 120 && row[x*4+1] < 120 && row[x*4+2] < 120)
				dark++;
	}
	return dark;
}

/* on failure: an 80-column ASCII downsample of the raster, into the log */
static void
dump(void)
{
	uchar row[400*4];
	char line[81];
	int x, y;

	for(y = 0; y < 120; y += 4){
		if(pread(rgbfd, row, sizeof row, (vlong)y*400*4) != sizeof row)
			return;
		for(x = 0; x < 80; x++){
			uchar *px = row + x*5*4;
			if(px[0] < 120 && px[1] < 120 && px[2] < 120)
				line[x] = '#';
			else if(px[0] > 245 && px[1] > 245 && px[2] > 245)
				line[x] = '.';
			else
				line[x] = '~';
		}
		line[80] = 0;
		print("|%s|\n", line);
	}
}

static void
chord(char *msg)
{
	char buf[64];
	int fd;

	strcpy(buf, winpath);
	strcat(buf, "/wctl");
	fd = open(buf, OWRITE);
	write(fd, msg, strlen(msg));
	close(fd);
}

int
main(int argc, char *argv[])
{
	char buf[128];
	int fd, n, i, booted, base, opened;

	USED(argc); USED(argv);
	fd = open("#w/clone", OREAD);
	n = fd >= 0 ? read(fd, buf, 15) : -1;
	buf[n > 0 ? n : 0] = 0;
	strcpy(winpath, "#w/");
	strcpy(winpath+3, buf);

	procrfork(RFFDG|RFNAMEG, acmechild, nil);

	strcpy(buf, winpath);
	strcat(buf, "/rgb");
	rgbfd = open(buf, OREAD);

	/* acme's first paint: the row tag and two column tags, inked */
	booted = 0;
	for(i = 0; i < 200 && !booted; i++){
		sleep(100);
		booted = darkin(0, 16) >= 25 && darkin(17, 33) >= 25;
	}
	ok(booted, "acme booted: row and column tags inked");
	if(!booted)
		dump();

	/* button 2 on "New" in the left column's tag: a window opens and
	 * its own tag inks the rows directly below the column tag */
	base = darkin(34, 64);
	chord("mouse 25 24 2");
	chord("mouse 25 24 0");
	opened = 0;
	for(i = 0; i < 100 && !opened; i++){
		sleep(100);
		opened = darkin(34, 64) >= base + 20;
	}
	ok(opened, "button-2 New in a column tag: a window opened, its tag inked");
	if(!opened)
		dump();

	/* button 3 on 'rc/' in the '/' directory window (the right column,
	 * whose body text sits right of the 242..254 scrollbar): Look
	 * resolves the name in the window's directory and opens /rc — a new
	 * window whose tag inks the right column's lower half */
	/* the split point where the new window lands varies a few lines
	 * between hosts (the column's height heuristic); the band is wide so
	 * the assertion is about the LOOK, not one host's layout */
	base = darkin(84, 240);
	chord("mouse 280 55 4");
	chord("mouse 280 55 0");
	opened = 0;
	for(i = 0; i < 100 && !opened; i++){
		sleep(100);
		opened = darkin(84, 240) >= base + 20;
	}
	ok(opened, "button-3 Look on rc/: the directory opened in a new window");
	if(!opened)
		dump();

	if(nfail == 0)
		print("acmetest: all %d passed\n", npass);
	exits(nfail ? "fail" : "");
	return 0;
}
