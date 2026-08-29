/* sha256sum: hex digest of stdin, or of each named file ("hash  name").
 * The digest pkg(1) trusts; the suite uses it to author local registries. */
#include "lib9.h"

static void
hashfd(int fd, char *name)
{
	SHA256state s;
	uchar buf[8192], out[32];
	char hex[65];
	int n, i;

	sha256init(&s);
	while((n = read(fd, buf, sizeof buf)) > 0)
		sha256update(&s, buf, n);
	sha256final(&s, out);
	for(i = 0; i < 32; i++){
		hex[i*2]   = "0123456789abcdef"[out[i] >> 4];
		hex[i*2+1] = "0123456789abcdef"[out[i] & 15];
	}
	hex[64] = 0;
	if(name)
		print("%s  %s\n", hex, name);
	else
		print("%s\n", hex);
}

int
main(int argc, char *argv[])
{
	int i, fd;

	if(argc == 1){
		hashfd(0, nil);
		exits(nil);
	}
	for(i = 1; i < argc; i++){
		fd = open(argv[i], OREAD);
		if(fd < 0){
			fprint(2, "sha256sum: cannot open %s\n", argv[i]);
			exits("open");
		}
		hashfd(fd, argv[i]);
		close(fd);
	}
	exits(nil);
	return 0;
}
