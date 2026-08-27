/* wasitest: a citizen of the OTHER world — compiled against wasi-libc
 * through the full wasm32-wasip1 sysroot, importing wasi_snapshot_preview1,
 * knowing nothing of Plan 9. The shim (supervisor/wasi1.mjs) maps its world
 * onto the namespace: the preopen is /. Deliberately ordinary C. */
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <dirent.h>

int
main(int argc, char **argv)
{
	FILE *f;
	DIR *d;
	struct dirent *e;
	char buf[256];
	int n;

	printf("wasi: hello from wasi-libc\n");
	printf("wasi: argc=%d argv1=%s\n", argc, argc > 1 ? argv[1] : "(none)");
	printf("wasi: clock=%s\n", time(NULL) > 0 ? "ticking" : "dead");

	f = fopen("/etc/motd", "r");
	if (f && fgets(buf, sizeof buf, f)) {
		printf("wasi: motd: %s", buf);
		fclose(f);
	}

	f = fopen("/tmp/wasi.out", "w");
	if (f) {
		fprintf(f, "written by wasi\n");
		fclose(f);
	}
	f = fopen("/tmp/wasi.out", "r");
	if (f && fgets(buf, sizeof buf, f)) {
		printf("wasi: readback: %s", buf);
		fclose(f);
	}

	n = 0;
	d = opendir("/etc");
	if (d) {
		while ((e = readdir(d)) != NULL)
			n++;
		closedir(d);
	}
	printf("wasi: /etc has %d entries\n", n);
	return 0;
}
