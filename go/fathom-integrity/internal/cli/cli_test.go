package cli

import (
	"bytes"
	"os"
	"path/filepath"
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

func TestMainRecoverRequiresDBAndDest(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"recover"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db and --dest are required")
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

func TestMainRebuildMapsBadRequestToUsageExitCode(t *testing.T) {
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"invalid target","error_code":"bad_request","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "invalid target")
}

func TestMainRebuildMapsIntegrityFailureToExitCodeFour(t *testing.T) {
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"integrity failed","error_code":"integrity_failure","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 4, exitCode)
	require.Contains(t, stderr.String(), "integrity failed")
}

func TestMainRebuildMapsUnsupportedCapabilityToExitCodeThree(t *testing.T) {
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"sqlite-vec unavailable","error_code":"unsupported_capability","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 3, exitCode)
