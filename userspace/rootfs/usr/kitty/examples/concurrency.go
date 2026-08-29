// Goroutines, channels, select, timeouts, atomics, WaitGroups, sorting —
// gobyexample.com's concurrency chapters, on a kernel in a browser tab:
//   go run concurrency.go
package main

import (
	"fmt"
	"slices"
	"sort"
	"sync"
	"sync/atomic"
	"time"
)

func main() {
	c1 := make(chan string)
	go func() { time.Sleep(10 * time.Millisecond); c1 <- "one" }()
	select {
	case msg := <-c1:
		fmt.Println("select:", msg)
	case <-time.After(time.Second):
		fmt.Println("timeout (unexpected)")
	}
	var ops atomic.Uint64
	var wg sync.WaitGroup
	for range 10 {
		wg.Add(1)
		go func() { defer wg.Done(); ops.Add(1) }()
	}
	wg.Wait()
	fmt.Println("atomic:", ops.Load())
	s := []string{"kiwi", "apple", "mango"}
	sort.Strings(s)
	fmt.Println("sorted:", s, slices.Contains(s, "mango"))
}
