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
	sqlitePath := testutil.SQLiteBinary()

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
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)

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
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedExciseScenario(t, dbPath)

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

	activeRow := queryDB(t, dbPath, "SELECT row_id FROM nodes WHERE logical_id='meeting-1' AND superseded_at IS NULL")
	require.Equal(t, "row-1", activeRow, "prior version should be restored as active")
}

func TestRebuildCommandRepairsMissingFTS(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedFTSRepairScenario(t, dbPath)

	// Confirm FTS is empty before rebuild.
	before := queryDB(t, dbPath, "SELECT count(*) FROM fts_nodes")
	require.Equal(t, "0", before, "FTS should be empty before rebuild")

	cmd := exec.Command(
		"go",
		"run",
		"./cmd/fathom-integrity",
		"rebuild",
		"--db", dbPath,
		"--bridge", bridgePath,
		"--target", "fts",
	)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "projection rebuild completed")

	after := queryDB(t, dbPath, "SELECT count(*) FROM fts_nodes")
	require.Equal(t, "1", after, "FTS should have one row after rebuild")
}

func TestCheckCommandOnCleanDB(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "check", "--db", dbPath)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"overall":"clean"`)
}

func TestCheckDetectsStaleFTS(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedFTSRepairScenario(t, dbPath)

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "check", "--db", dbPath)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"stale_fts_rows":1`)
	require.Contains(t, string(output), `"overall":"degraded"`)
}

func TestCheckDetectsNullSourceRef(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	testutil.InjectNullSourceRef(t, dbPath)

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "check", "--db", dbPath)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"null_source_ref_nodes":1`)
	require.Contains(t, string(output), `"overall":"degraded"`)
}

func TestCheckDetectsOrphanedChunk(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	testutil.InjectOrphanedChunk(t, dbPath)

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "check", "--db", dbPath)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"orphaned_chunks":1`)
	require.Contains(t, string(output), `"overall":"degraded"`)
}

// --- helpers ---

func makeBridgeScript(t *testing.T, tempDir, repoRoot string) string {
	t.Helper()
	bridgePath := filepath.Join(tempDir, "bridge.sh")
	script := "#!/usr/bin/env bash\nset -euo pipefail\ncd " + repoRoot + "\ncargo run --quiet -p fathomdb-engine --bin fathomdb-admin-bridge\n"
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))
	return bridgePath
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

func queryDB(t *testing.T, dbPath, query string) string {
	t.Helper()
	cmd := exec.Command(testutil.SQLiteBinary(), dbPath, query)
	cmd.Env = os.Environ()
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
	return strings.TrimSpace(string(output))
}
