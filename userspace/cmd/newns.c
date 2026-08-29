/* newns: run a command in a namespace built from a file (auth/newns's
 * shape). The namespace is cleared and rebuilt from the file's text — so
 * an edited file re-shapes the world with no recompile, which is M2's
 * acceptance and the profile's mechanism in miniature.
 *
 *   newns file [cmd arg...]      default cmd: rc
 */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char *a[32], path[256];
	int i, na = 0;

	if(argc < 2){
		fprint(2, "usage: newns file [cmd arg...]\n");
		exits("usage");
	}
	if(newns(argv[1]) < 0)
		fprint(2, "newns: %s applied with errors\n", argv[1]);
	if(argc == 2){
		a[na++] = "rc";
		a[na] = nil;
		exec("/bin/rc", a);
		exits("exec");
	}
	for(i = 2; i < argc && na < 31; i++)
		a[na++] = argv[i];
	a[na] = nil;
	if(strchr(a[0], '/'))
		strecpy(path, path + sizeof path, a[0]);
	else
		snprint(path, sizeof path, "/bin/%s", a[0]);
	exec(path, a);
	fprint(2, "newns: cannot exec %s\n", path);
	exits("exec");
	return 0;
}
