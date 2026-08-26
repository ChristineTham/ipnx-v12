/* dtest: the window server's acceptance tests, headless. Mints a window,
 * binds it over /dev (a namespace copy — this process IS the window),
 * paints through /dev/draw, and asserts pixels through the v0 rgb file. */
#include "lib9.h"
#include "draw9.h"

static int npass, nfail;
static void ok(int cond, char *what){
	if(cond){ npass++; print("PASS %s\n", what); }
	else    { nfail++; print("FAIL %s\n", what); }
}

static int rgbfd;

static ulong
pixel(Draw *d, int x, int y)
{
	uchar px[4];

	if(pread(rgbfd, px, 4, (vlong)(y*d->w + x)*4) != 4)
		return 0xDEAD;
	return (ulong)px[0]<<24 | px[1]<<16 | px[2]<<8 | px[3];
}

int
main(int argc, char *argv[])
{
	char buf[128], path[32];
	Draw d;
	int fd, n, red, blue;

	USED(argc); USED(argv);
	/* keep the console for PASS lines; the window lands at /n/win */
	fd = open("#w/clone", OREAD);
	n = fd >= 0 ? read(fd, buf, 15) : -1;
	buf[n > 0 ? n : 0] = 0;
	ok(n > 0, "wsys: reading clone mints a window");
	strcpy(path, "#w/");
	strcpy(path+3, buf);
	bind(path, "/n/win", MREPL);

	ok(drawattach(&d, "/n/win") == 0 && d.w == 400 && d.h == 300,
	   "draw: new answers twelve fields; the window is 400x300");

	rgbfd = open("/n/win/rgb", OREAD);
	red = drawcolor(&d, 255, 0, 0, 255);
	blue = drawcolor(&d, 0, 0, 255, 255);
	drawrect(&d, 50, 50, 150, 120, red);
	drawline(&d, 0, 0, 399, 299, blue);
	drawellipse(&d, 300, 80, 40, 40, blue, 1);
	drawflush(&d);
	ok(pixel(&d, 100, 85) == 0xFF0000FF, "draw: d fills a rectangle (red at 100,85)");
	ok(pixel(&d, 200, 150) == 0x0000FFFF, "draw: L draws the diagonal (blue at 200,150)");
	ok(pixel(&d, 300, 80) == 0x0000FFFF, "draw: E fills an ellipse (blue at centre)");
	ok(pixel(&d, 10, 200) == 0xFFFFFFFF, "draw: the background is untouched white");

	fd = open("/n/win/wctl", ORDWR);
	n = read(fd, buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	ok(strstr(buf, "400 300 current visible") != nil,
	   "wctl: geometry reads back");
	n = fprint(fd, "resize 200 100");
	close(fd);
	fd = open("/n/win/wctl", OREAD);
	n = read(fd, buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strstr(buf, "200 100 current visible") != nil,
	   "wctl: resize applies");

	fd = open("/n/win/label", ORDWR);
	fprint(fd, "dtest was here");
	close(fd);
	fd = open("/n/win/label", OREAD);
	n = read(fd, buf, sizeof buf - 1);
	buf[n > 0 ? n : 0] = 0;
	close(fd);
	ok(strcmp(buf, "dtest was here") == 0, "label: writes read back");

	fd = open("/n/win/wctl", OWRITE);
	fprint(fd, "delete");
	close(fd);

	if(nfail)
		print("dtest: %d FAILED\n", nfail);
	return nfail;
}
