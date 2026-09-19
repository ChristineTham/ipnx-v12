#include <u.h>
#include <libc.h>

void
main(int argc, char *argv[])
{
	char *buf, *p, *ep;
	int i, nflag;

	nflag = 0;
	if(argc > 1 && strcmp(argv[1], "-n") == 0){
		nflag = 1;
		argc--;
		argv++;
	}
	buf = malloc(8192);
	if(buf == nil)
		sysfatal("no memory");
	ep = buf+8192;
	p = buf;
	for(i = 1; i < argc; i++){
		if(i > 1 && p < ep)
			*p++ = ' ';
		p = strecpy(p, ep, argv[i]);
	}
	if(!nflag && p < ep)
		*p++ = '\n';
	if(write(1, buf, p-buf) != p-buf)
		sysfatal("write error: %r");
	exits(nil);
}
