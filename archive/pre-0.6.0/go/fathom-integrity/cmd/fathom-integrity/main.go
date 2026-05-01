package main

import (
	"os"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/cli"
)

func main() {
	os.Exit(cli.Main(os.Args[1:], os.Stdout, os.Stderr))
}
