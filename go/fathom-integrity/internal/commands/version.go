package commands

import (
	"fmt"
	"io"
)

const Version = "0.1.0"

func RunVersion(out io.Writer) error {
	_, err := fmt.Fprintf(out, "fathom-integrity %s\n", Version)
	return err
}
