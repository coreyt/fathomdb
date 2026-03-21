package commands

import (
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

const Version = "0.1.0"

func RunVersion(out io.Writer) error {
	_, err := fmt.Fprintf(out, "fathom-integrity %s (admin protocol %d)\n", Version, bridge.ProtocolVersion)
	return err
}
