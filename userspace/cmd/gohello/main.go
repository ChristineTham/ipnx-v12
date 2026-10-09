// gohello — a Go program, built for WASI preview 1 by Go's own toolchain
// (GOOS=wasip1 GOARCH=wasm) and run by the system unmodified, as a WASI
// program (docs/architecture.md, "A WASI binary runs natively"). It is the
// previous demo's, and prints what it printed there.
package main

import "fmt"

func main() {
	fmt.Println("Hello Kitty — from Go (GOOS=wasip1, unmodified)")
}
