/* go: the Go toolchain compiles wasm on the host (its compiler is not a
 * wasm binary), but real Go binaries — GOOS=wasip1 — run here as first-class
 * WASI guests. `go` runs the sample so the interop is visible; `gotest` is
 * another. This is the honest shape of Go on ipnx (docs/design.md). */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char *a[] = { "gohello", nil };

	USED(argc); USED(argv);
	fprint(2, "go: the Go compiler runs on the host (GOOS=wasip1 go build);\n");
	fprint(2, "    ipnx runs the binaries. here is a real Go program:\n\n");
	exec("/bin/gohello", a);
	fprint(2, "go: gohello is not installed\n");
	exits("exec");
	return 0;
}
