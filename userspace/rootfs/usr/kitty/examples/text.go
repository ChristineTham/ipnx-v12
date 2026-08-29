// regexp, text/template, base64, URL parsing, time formatting, args —
// gobyexample.com's text chapters:
//   go run text.go some args
package main

import (
	"encoding/base64"
	"fmt"
	"net/url"
	"os"
	"regexp"
	"text/template"
	"time"
)

func main() {
	r := regexp.MustCompile(`p([a-z]+)ch`)
	fmt.Println("regexp:", r.FindString("peach punch"), r.ReplaceAllString("a peach", "<fruit>"))
	t := template.Must(template.New("t").Parse("template: {{range .}}[{{.}}]{{end}}\n"))
	t.Execute(os.Stdout, []string{"Go", "on", "ipnx"})
	enc := base64.StdEncoding.EncodeToString([]byte("hello kitty"))
	dec, _ := base64.StdEncoding.DecodeString(enc)
	fmt.Println("base64:", enc, string(dec))
	u, _ := url.Parse("https://kitty:meow@christham.net:443/ipnx-v12?lens=four#top")
	fmt.Println("url:", u.Scheme, u.Host, u.Path, u.Query().Get("lens"))
	fmt.Println("time:", time.Date(2026, 8, 29, 0, 0, 0, 0, time.UTC).Format("Mon Jan 2 2006"))
	fmt.Println("args:", os.Args[1:])
}
