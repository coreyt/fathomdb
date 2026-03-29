package commands

import (
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

// Version is the current release version of the fathom-integrity CLI.
const Version = "0.1.0"

// RunVersion prints the fathom-integrity version and protocol version to out.
func RunVersion(out io.Writer) error {
	_, err := fmt.Fprintf(out, "fathom-integrity %s (admin protocol %d)\n", Version, bridge.ProtocolVersion)
	return err
}
