// The worker-pool pattern: jobs and results over channels, three
// goroutines, JSON out, a sha256 —
//   go build workers.go ; ./workers
package main

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"sync"
)

func worker(id int, jobs <-chan int, results chan<- int, wg *sync.WaitGroup) {
	defer wg.Done()
	for j := range jobs {
		results <- j * j
	}
	_ = id
}

func main() {
	jobs := make(chan int, 5)
	results := make(chan int, 5)
	var wg sync.WaitGroup
	for w := 1; w <= 3; w++ {
		wg.Add(1)
		go worker(w, jobs, results, &wg)
	}
	for j := 1; j <= 5; j++ {
		jobs <- j
	}
	close(jobs)
	wg.Wait()
	close(results)
	sum := 0
	for r := range results {
		sum += r
	}
	b, _ := json.Marshal(map[string]any{"sum": sum, "workers": 3})
	fmt.Println(string(b))
	h := sha256.Sum256([]byte("kitty"))
	fmt.Printf("sha256(kitty) = %x...\n", h[:8])
}
