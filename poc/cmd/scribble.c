/* scribble: draw where the mouse drags; right-click clears. The
 * interactive proof that a window's mouse file feeds its namespace. */
#include "lib9.h"
#include "draw9.h"

int
main(int argc, char *argv[])
{
	char m[64];
	Draw d;
	int fd, n, x, y, b, ink, bg;

	USED(argc); USED(argv);
	if(drawinit(&d) < 0)
		exits("draw");
	fd = open("/dev/mouse", OREAD);
	if(fd < 0){
		fprint(2, "scribble: no /dev/mouse: %r\n");
		exits("mouse");
	}
	bg = drawcolor(&d, 250, 250, 245, 255);
	ink = drawcolor(&d, 40, 60, 180, 255);
	drawrect(&d, 0, 0, d.w, d.h, bg);
	drawflush(&d);
	for(;;){
		n = read(fd, m, 49);
		if(n <= 0)
			break;
		m[n] = 0;
		x = atoi(m+1);
		y = atoi(m+13);
		b = atoi(m+25);
		if(b & 1){
			drawellipse(&d, x, y, 3, 3, ink, 1);
			drawflush(&d);
		}
		if(b & 4){
			drawrect(&d, 0, 0, d.w, d.h, bg);
			drawflush(&d);
		}
	}
	return 0;
}
