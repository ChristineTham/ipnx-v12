/* dtext: text in a window — the s message at work. Run it as: win dtext */
#include "lib9.h"
#include "draw9.h"

int
main(int argc, char *argv[])
{
	Draw d;
	int bg, ink, dim, font, y;
	static char *lines[] = {
		"ipnx-v12: text lands in /dev/draw",
		"",
		"the s message, an 8x8 font carried",
		"by y, i and l -- what sam waits on.",
		nil,
	};
	int i;

	USED(argc); USED(argv);
	if(drawinit(&d) < 0)
		exits("draw");
	bg = drawcolor(&d, 251, 250, 246, 255);
	ink = drawcolor(&d, 30, 32, 40, 255);
	dim = drawcolor(&d, 140, 120, 90, 255);
	drawrect(&d, 0, 0, d.w, d.h, bg);
	font = drawfontinit(&d);
	y = 40;
	for(i = 0; lines[i] != nil; i++){
		drawtext(&d, 24, y, lines[i], font, i == 0 ? ink : dim);
		y += 14;
	}
	drawline(&d, 24, 50, 24 + 8*33, 50, dim);
	drawflush(&d);
	for(;;)
		sleep(10000);
	return 0;
}
