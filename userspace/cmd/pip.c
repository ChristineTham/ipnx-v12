/* pip: entry to the Python personality's package installer.
 * The installer is /lib/python3.14/pip.py — on sys.path, so `python -m pip`
 * is the same program.
 * A wrapper because scripts are not executable here (no shebang yet);
 * the installer itself is Python, running on the real CPython.
 */
#include "lib9.h"

enum { MAXARG = 64 };

int
main(int argc, char *argv[])
{
	char *a[MAXARG];
	int i, na = 0;

	a[na++] = "python";
	a[na++] = "/lib/python3.14/pip.py";
	for(i = 1; i < argc && na < MAXARG-1; i++) a[na++] = argv[i];
	a[na] = nil;
	exec("/bin/python", a);
	fprint(2, "pip: cannot exec /bin/python\n");
	exits("exec");
	return 0;
}
