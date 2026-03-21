package cli

import (
	"bytes"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestMainRequiresCommand(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main(nil, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "usage:")
}

func TestMainVersionCommand(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"version"}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	require.Contains(t, stdout.String(), "fathom-integrity 0.1.0")
	require.Contains(t, stdout.String(), "admin protocol 1")
	require.Empty(t, stderr.String())
}
