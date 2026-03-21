package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestVersionCommand(t *testing.T) {
	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "version")
	cmd.Dir = filepath.Join("..", "..")
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "fathom-integrity 0.1.0")
}
