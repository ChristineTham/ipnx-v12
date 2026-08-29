/* go: an honest Go front-end for ipnx.
 *
 * Go's toolchain compiles wasm on the HOST, not in the tab. Two reasons, both
 * real: `go build` orchestrates its compiler/assembler/linker through
 * os/exec, and Go's wasip1 runtime has no process spawning (WASI omits it);
 * and the compiler itself is not shipped as a wasm binary. So `go build`/`go
 * run` cannot run here — but real Go BINARIES (GOOS=wasip1) do, as first-class
 * WASI guests. This front-end tells the truth and runs them.
 *
 *   go version      the toolchain this system's binaries were built with
 *   go env [VAR]    GOOS/GOARCH and friends
 *   go run f.go     honestly declines (compiler is host-side); runs the demo
 *   gohello, gotest real Go programs, already built — run them directly
 */
#include "lib9.h"

static char *ver = "go version go1.25.6 wasip1/wasm";

int
main(int argc, char *argv[])
{
	char *cmd = argc > 1 ? argv[1] : "";

	if(strcmp(cmd, "version") == 0){
		print("%s\n", ver);
		exits(nil);
	}
	if(strcmp(cmd, "env") == 0){
		char *v = argc > 2 ? argv[2] : nil;
		if(v == nil){
			print("GOOS='wasip1'\nGOARCH='wasm'\nGOVERSION='go1.25.6'\n");
			print("GOROOT='host'  # the toolchain lives on the host\n");
		} else if(strcmp(v,"GOOS")==0) print("wasip1\n");
		else if(strcmp(v,"GOARCH")==0) print("wasm\n");
		else if(strcmp(v,"GOVERSION")==0) print("go1.25.6\n");
		else print("\n");
		exits(nil);
	}
	if(strcmp(cmd, "run") == 0 || strcmp(cmd, "build") == 0){
		char *a[] = { "gohello", nil };
		fprint(2, "go %s: Go's compiler runs on the host, not in the tab —\n", cmd);
		fprint(2, "  its toolchain drives compile/link through os/exec, which\n");
		fprint(2, "  WASI (and Go's wasip1 runtime) does not provide. C compiles\n");
		fprint(2, "  here because clang is a single wasm binary; Go's gc is not.\n");
		fprint(2, "  But real Go binaries run — here is one (built GOOS=wasip1):\n\n");
		exec("/bin/gohello", a);
		exits("exec");
	}
	fprint(2, "usage: go version | env [VAR] | run f.go\n");
	fprint(2, "note: Go compiles on the host; run built Go binaries directly\n");
	fprint(2, "      (gohello, gotest). why: `go help ipnx` — no, really, read cc(1).\n");
	exits(nil);
	return 0;
}
