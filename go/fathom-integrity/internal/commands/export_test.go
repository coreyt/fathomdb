package commands

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

// makeFakeBridge writes a shell script that emits a fixed bridge response and
// returns its path. script must be a valid shell script body (without shebang).
func makeFakeBridge(t *testing.T, responseJSON string) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "fake-bridge.sh")
	script := "#!/usr/bin/env bash\nprintf '%s\\n' '" + responseJSON + "'\n"
	require.NoError(t, os.WriteFile(path, []byte(script), 0o755))
	return path
}

func TestRunExport_FailsWithoutBridgePath(t *testing.T) {
	var out bytes.Buffer
	err := RunExport("some.db", "/tmp/out.db", "", &out)
	require.Error(t, err)
	require.Contains(t, err.Error(), "--bridge")
}

func TestRunExport_BridgeBackedExport(t *testing.T) {
	destDir := t.TempDir()
	destPath := filepath.Join(destDir, "backup.db")

	manifest := `{"protocol_version":1,"ok":true,"message":"export created","payload":{"exported_at":1742741234,"sha256":"a3f1c2d4e5b6a7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2","schema_version":1,"protocol_version":1,"page_count":32}}`
	bridge := makeFakeBridge(t, manifest)

	var out bytes.Buffer
	err := RunExport("fathom.db", destPath, bridge, &out)

	require.NoError(t, err)
	require.Contains(t, out.String(), "sha256")
	require.Contains(t, out.String(), "pages")
	require.Contains(t, out.String(), "schema")
}

func TestRunExport_FailsWhenBridgeReturnsError(t *testing.T) {
	destDir := t.TempDir()
	destPath := filepath.Join(destDir, "backup.db")

	errorResponse := `{"protocol_version":1,"ok":false,"message":"checkpoint blocked","payload":{}}`
	bridge := makeFakeBridge(t, errorResponse)

	var out bytes.Buffer
	err := RunExport("fathom.db", destPath, bridge, &out)

	require.Error(t, err)
	require.Contains(t, err.Error(), "checkpoint blocked")
}
