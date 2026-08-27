// gotest: a REAL Go binary — GOOS=wasip1 GOARCH=wasm, the first benchmark's
// own toolchain — running against the hosted kernel through wasi1.mjs.
// Files, directories, timers: everything it sees is the namespace.
package main

import (
	"fmt"
	"os"
	"time"
)

func main() {
	fmt.Println("go: hello from wasip1")

	b, err := os.ReadFile("/etc/motd")
	if err != nil {
		fmt.Println("go: motd error:", err)
		os.Exit(1)
	}
	fmt.Print("go: motd: ", string(b))

	t0 := time.Now()
	time.Sleep(50 * time.Millisecond)
	fmt.Printf("go: slept=%v\n", time.Since(t0) >= 40*time.Millisecond)

	es, err := os.ReadDir("/etc")
	if err != nil {
		fmt.Println("go: readdir error:", err)
		os.Exit(1)
	}
	fmt.Printf("go: /etc has %d entries\n", len(es))

	if err := os.WriteFile("/tmp/go.out", []byte("written by go\n"), 0644); err != nil {
		fmt.Println("go: write error:", err)
		os.Exit(1)
	}
	c, _ := os.ReadFile("/tmp/go.out")
	fmt.Print("go: readback: ", string(c))
}
