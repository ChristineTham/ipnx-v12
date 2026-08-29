// Closures, recursion, generics, interfaces, methods —
// the language features gobyexample.com opens with, runnable here:
//   go run basics.go
package main

import "fmt"

type geometry interface{ area() float64 }
type rect struct{ w, h float64 }

func (r rect) area() float64 { return r.w * r.h }

func mapper[T, U any](s []T, f func(T) U) []U {
	out := make([]U, len(s))
	for i, v := range s {
		out[i] = f(v)
	}
	return out
}

func intSeq() func() int {
	i := 0
	return func() int { i++; return i }
}

func fact(n int) int {
	if n == 0 {
		return 1
	}
	return n * fact(n-1)
}

func main() {
	next := intSeq()
	next()
	next()
	fmt.Println("closure:", next())
	fmt.Println("recursion:", fact(7))
	fmt.Println("generics:", mapper([]int{1, 2, 3}, func(x int) string { return fmt.Sprint(x * x) }))
	var g geometry = rect{3, 4}
	fmt.Println("interface:", g.area())
}
