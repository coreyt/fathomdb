package e2e

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

func TestVersionCommand(t *testing.T) {
	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "version")
	cmd.Dir = filepath.Join("..", "..")
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "fathom-integrity 0.1.0")
	require.Contains(t, string(output), "admin protocol 1")
}

func TestE2ESQLiteBinarySupportsUnixepoch(t *testing.T) {
	sqlitePath := sqliteBinary(t)

	cmd := exec.Command(sqlitePath, ":memory:", "select unixepoch();")
	cmd.Dir = filepath.Join("..", "..")
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
}

func TestRepoSQLitePolicyLoadsExpectedVersion(t *testing.T) {
	policy, err := testutil.LoadSQLitePolicy()

	require.NoError(t, err)
	require.Equal(t, "3.41.0", policy.MinimumSupportedVersion)
	require.Equal(t, "3.46.0", policy.RepoDevVersion)
	require.Contains(t, policy.RepoLocalBinaryRelPath, "sqlite-3.46.0/bin/sqlite3")
}

func TestTraceCommandAgainstRealBridgeAndTempDB(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := filepath.Join(tempDir, "bridge.sh")

	bridgeScript := "#!/usr/bin/env bash\nset -euo pipefail\ncd " + repoRoot + "\ncargo run --quiet -p fathomdb-engine --bin fathomdb-admin-bridge\n"
	require.NoError(t, os.WriteFile(bridgePath, []byte(bridgeScript), 0o755))

	bootstrapBridgeDB(t, bridgePath, dbPath)
	seedTraceScenario(t, repoRoot, dbPath)

	cmd := exec.Command(
		"go",
		"run",
		"./cmd/fathom-integrity",
		"trace",
		"--db", dbPath,
		"--bridge", bridgePath,
		"--source-ref", "source-1",
	)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "trace completed")
	require.Contains(t, string(output), `"source_ref":"source-1"`)
	require.Contains(t, string(output), `"node_rows":1`)
	require.Contains(t, string(output), `"action_rows":1`)
}

func TestExciseCommandRestoresPriorVersion(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := filepath.Join(tempDir, "bridge.sh")

	bridgeScript := "#!/usr/bin/env bash\nset -euo pipefail\ncd " + repoRoot + "\ncargo run --quiet -p fathomdb-engine --bin fathomdb-admin-bridge\n"
	require.NoError(t, os.WriteFile(bridgePath, []byte(bridgeScript), 0o755))

	bootstrapBridgeDB(t, bridgePath, dbPath)
	seedExciseScenario(t, repoRoot, dbPath)

	cmd := exec.Command(
		"go",
		"run",
		"./cmd/fathom-integrity",
		"excise",
		"--db", dbPath,
		"--bridge", bridgePath,
		"--source-ref", "source-2",
	)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "source excised")

	// Verify that the prior version (source-1) is now active.
	activeRow := queryDB(t, dbPath, "SELECT row_id FROM nodes WHERE logical_id='meeting-1' AND superseded_at IS NULL")
	require.Equal(t, "row-1", activeRow, "prior version should be restored as active")
}

func seedExciseScenario(t *testing.T, repoRoot, dbPath string) {
	t.Helper()

	// Insert v1 (source-1) already superseded, then v2 (source-2) active.
	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 200, 'source-1');
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-2', 'meeting-1', 'Meeting', '{}', 200, 'source-2');
`
	cmd := exec.Command(testutil.SQLiteBinary(), dbPath, sql)
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
}

func queryDB(t *testing.T, dbPath, query string) string {
	t.Helper()
	cmd := exec.Command(testutil.SQLiteBinary(), dbPath, query)
	cmd.Env = os.Environ()
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
	return strings.TrimSpace(string(output))
}

func bootstrapBridgeDB(t *testing.T, bridgePath, dbPath string) {
	t.Helper()

	requestBody, err := json.Marshal(map[string]any{
		"protocol_version": 1,
		"database_path":    dbPath,
		"command":          "check_integrity",
	})
	require.NoError(t, err)

	cmd := exec.Command(bridgePath)
	cmd.Stdin = bytes.NewReader(requestBody)
	cmd.Dir = filepath.Join("..", "..")
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
	require.Contains(t, string(output), `"protocol_version":1`)
}

func seedTraceScenario(t *testing.T, repoRoot, dbPath string) {
	t.Helper()

	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', unixepoch(), 'source-1');
INSERT INTO runs (id, kind, status, properties, created_at, source_ref)
VALUES ('run-1', 'session', 'completed', '{}', unixepoch(), 'source-1');
INSERT INTO steps (id, run_id, kind, status, properties, created_at, source_ref)
VALUES ('step-1', 'run-1', 'llm', 'completed', '{}', unixepoch(), 'source-1');
INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref)
VALUES ('action-1', 'step-1', 'emit', 'completed', '{}', unixepoch(), 'source-1');
`

	cmd := exec.Command(testutil.SQLiteBinary(), dbPath, sql)
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
}

func sqliteBinary(t *testing.T) string {
	t.Helper()
	return testutil.SQLiteBinary()
}
