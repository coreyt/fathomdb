package e2e

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

func TestVersionCommand(t *testing.T) {
	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "version")
	cmd.Dir = filepath.Join("..", "..")
	cmd.Env = commandEnv(t)

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
	cmd.Env = commandEnv(t)

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
	cmd.Env = commandEnv(t)

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
	cmd.Env = commandEnv(t)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "projection rebuild completed")

	after := queryDB(t, dbPath, "SELECT count(*) FROM fts_nodes")
	require.Equal(t, "1", after, "FTS should have one row after rebuild")
}

func TestRebuildMissingCommandRepairsMissingFTS(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedFTSRepairScenario(t, dbPath)

	before := queryDB(t, dbPath, "SELECT count(*) FROM fts_nodes")
	require.Equal(t, "0", before, "FTS should be empty before rebuild-missing")

	cmd := exec.Command(
		"go",
		"run",
		"./cmd/fathom-integrity",
		"rebuild-missing",
		"--db", dbPath,
		"--bridge", bridgePath,
	)
	cmd.Dir = repoRoot
	cmd.Env = commandEnv(t)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "missing projection rebuild completed")

	after := queryDB(t, dbPath, "SELECT count(*) FROM fts_nodes")
	require.Equal(t, "1", after, "FTS should have one row after rebuild-missing")
}

func TestRebuildCommandRejectsInvalidTarget(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)

	cmd := exec.Command(
		"go",
		"run",
		"./cmd/fathom-integrity",
		"rebuild",
		"--db", dbPath,
		"--bridge", bridgePath,
		"--target", "weird",
	)
	cmd.Dir = repoRoot
	cmd.Env = commandEnv(t)

	output, err := cmd.CombinedOutput()

	require.Error(t, err, string(output))
	require.Contains(t, string(output), "invalid projection target")
}

func TestCheckLayer2DetectsBrokenStepFK(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.InjectBrokenStepFK(t, dbPath)

	cmd := exec.Command(
		"go", "run", "./cmd/fathom-integrity",
		"check",
		"--db", dbPath,
		"--bridge", bridgePath,
	)
	cmd.Dir = repoRoot
	cmd.Env = commandEnv(t)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"broken_step_fk":1`)
	require.Contains(t, string(output), `"overall":"corrupted"`)
}

func TestCheckLayer2DetectsDuplicateActive(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	testutil.InjectBrokenSupersession(t, dbPath)

	cmd := exec.Command(
		"go", "run", "./cmd/fathom-integrity",
		"check",
		"--db", dbPath,
		"--bridge", bridgePath,
	)
	cmd.Dir = repoRoot
	cmd.Env = commandEnv(t)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"duplicate_active_logical_ids":1`)
	require.Contains(t, string(output), `"overall":"corrupted"`)
}

func TestRepairCommandRepairsKnownCorruptionClasses(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	queryDB(t, dbPath, `
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('chunk-1', 'meeting-1', 'budget discussion', unixepoch());
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('chunk-1', 'meeting-1', 'Meeting', 'budget discussion');
`)
	testutil.InjectBrokenSupersession(t, dbPath)
	testutil.InjectBrokenStepFK(t, dbPath)
	testutil.InjectBrokenActionFK(t, dbPath)
	testutil.InjectOrphanedChunk(t, dbPath)

	repairCmd := exec.Command(
		"go", "run", "./cmd/fathom-integrity",
		"repair",
		"--db", dbPath,
		"--target", "all",
	)
	repairCmd.Dir = repoRoot
	repairCmd.Env = commandEnv(t)

	repairOutput, err := repairCmd.CombinedOutput()
	require.NoError(t, err, string(repairOutput))
	require.Contains(t, string(repairOutput), "repair completed")
	require.Contains(t, string(repairOutput), `"rows_superseded":1`)
	require.Contains(t, string(repairOutput), `"steps_deleted":1`)
	require.Contains(t, string(repairOutput), `"actions_deleted":1`)
	require.Contains(t, string(repairOutput), `"chunks_deleted":1`)

	checkCmd := exec.Command(
		"go", "run", "./cmd/fathom-integrity",
		"check",
		"--db", dbPath,
		"--bridge", bridgePath,
	)
	checkCmd.Dir = repoRoot
	checkCmd.Env = commandEnv(t)

	checkOutput, err := checkCmd.CombinedOutput()
	require.NoError(t, err, string(checkOutput))
	require.Contains(t, string(checkOutput), `"duplicate_active_logical_ids":0`)
	require.Contains(t, string(checkOutput), `"broken_step_fk":0`)
	require.Contains(t, string(checkOutput), `"orphaned_chunks":0`)
	require.Contains(t, string(checkOutput), `"overall":"clean"`)
}

func TestCheckCommandOnCleanDB(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity", "check", "--db", dbPath)
	cmd.Dir = repoRoot
	cmd.Env = commandEnv(t)

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
	cmd.Env = commandEnv(t)

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
	cmd.Env = commandEnv(t)

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
	cmd.Env = commandEnv(t)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "check completed")
	require.Contains(t, string(output), `"orphaned_chunks":1`)
	require.Contains(t, string(output), `"overall":"degraded"`)
}

func TestExportCommand_RoundTrip(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "backup.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)

	cmd := exec.Command(
		"go", "run", "./cmd/fathom-integrity",
		"export",
		"--db", dbPath,
		"--out", destPath,
		"--bridge", bridgePath,
	)
	cmd.Dir = repoRoot
	cmd.Env = commandEnv(t)
	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.FileExists(t, destPath)

	manifestPath := destPath + ".export-manifest.json"
	require.FileExists(t, manifestPath)

	data, readErr := os.ReadFile(manifestPath)
	require.NoError(t, readErr)

	var manifest map[string]any
	require.NoError(t, json.Unmarshal(data, &manifest))
	require.Equal(t, float64(1), manifest["protocol_version"], "protocol_version must be 1")
	require.Greater(t, manifest["page_count"].(float64), float64(0), "page_count must be positive")

	currentSchemaVersion := queryDB(t, destPath, "SELECT max(version) FROM fathom_schema_migrations")
	require.Equal(t, currentSchemaVersion, fmt.Sprintf("%.0f", manifest["schema_version"]))

	sha, _ := manifest["sha256"].(string)
	require.Len(t, sha, 64, "sha256 must be 64 hex chars")

	require.Contains(t, string(output), "sha256")
	require.Contains(t, string(output), "pages")
	require.Contains(t, string(output), "schema")
}

// --- helpers ---

func makeBridgeScript(t *testing.T, tempDir, repoRoot string) string {
	t.Helper()
	bridgePath := filepath.Join(tempDir, "bridge.sh")
	script := "#!/usr/bin/env bash\nset -euo pipefail\ncd " + repoRoot + "\ncargo run --quiet -p fathomdb-engine --bin fathomdb-admin-bridge\n"
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))
	return bridgePath
}

func makeVecBridgeScript(t *testing.T, tempDir, repoRoot string) string {
	t.Helper()
	bridgePath := filepath.Join(tempDir, "bridge-vec.sh")
	script := "#!/usr/bin/env bash\nset -euo pipefail\ncd " + repoRoot + "\ncargo run --quiet -p fathomdb-engine --features sqlite-vec --bin fathomdb-admin-bridge\n"
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
	cmd.Env = commandEnv(t)

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

type pythonVectorSearchResult struct {
	WasDegraded bool     `json:"was_degraded"`
	LogicalIDs  []string `json:"logical_ids"`
}

var pythonFathomdbInstallOnce sync.Once
var pythonFathomdbInstallErr error

func ensurePythonFathomdb(t *testing.T, repoRoot string) {
	t.Helper()

	absRepoRoot, err := filepath.Abs(repoRoot)
	require.NoError(t, err)
	projectRoot := filepath.Clean(filepath.Join(absRepoRoot, "..", ".."))

	pythonFathomdbInstallOnce.Do(func() {
		probe := exec.Command("python3", "-c", "import fathomdb")
		probe.Env = append(commandEnv(t), "PYTHONPATH="+filepath.Join(projectRoot, "python"))
		if err := probe.Run(); err == nil {
			return
		}

		install := exec.Command(
			"python3",
			"-m",
			"pip",
			"install",
			"-e",
			filepath.Join(projectRoot, "python"),
			"--no-build-isolation",
		)
		install.Env = append(commandEnv(t), "PYTHONPATH="+filepath.Join(projectRoot, "python"))
		output, err := install.CombinedOutput()
		if err != nil {
			pythonFathomdbInstallErr = fmt.Errorf("install python fathomdb: %w: %s", err, output)
			return
		}

		probe = exec.Command("python3", "-c", "import fathomdb")
		probe.Env = append(commandEnv(t), "PYTHONPATH="+filepath.Join(projectRoot, "python"))
		output, err = probe.CombinedOutput()
		if err != nil {
			pythonFathomdbInstallErr = fmt.Errorf("import fathomdb after install: %w: %s", err, output)
		}
	})

	require.NoError(t, pythonFathomdbInstallErr)
}

func pythonVectorSearch(t *testing.T, repoRoot, dbPath string) pythonVectorSearchResult {
	t.Helper()
	ensurePythonFathomdb(t, repoRoot)
	absRepoRoot, err := filepath.Abs(repoRoot)
	require.NoError(t, err)
	projectRoot := filepath.Clean(filepath.Join(absRepoRoot, "..", ".."))

	script := `
import json
import pathlib
import sys

project_root = pathlib.Path(sys.argv[1])
db_path = sys.argv[2]
sys.path.insert(0, str(project_root / "python"))

from fathomdb import Engine

engine = Engine.open(db_path, vector_dimension=4)
rows = engine.nodes("Document").vector_search("[1.0, 0.0, 0.0, 0.0]", limit=1).execute()
print(json.dumps({
    "was_degraded": rows.was_degraded,
    "logical_ids": [node.logical_id for node in rows.nodes],
}))
`

	cmd := exec.Command("python3", "-c", script, projectRoot, dbPath)
	cmd.Env = append(commandEnv(t), "PYTHONPATH="+filepath.Join(projectRoot, "python"))
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))

	var result pythonVectorSearchResult
	require.NoError(t, json.Unmarshal(output, &result))
	return result
}
