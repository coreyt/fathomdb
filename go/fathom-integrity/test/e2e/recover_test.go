package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

func TestRecoverCommand_CleanDBRoundTrip(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.Contains(t, string(output), `"recovered_db"`)
	require.Contains(t, string(output), `"row_counts"`)
	require.Contains(t, string(output), `"nodes":1`)
	require.Contains(t, string(output), `"overall":`)
	require.FileExists(t, destPath)
}

func TestRecoverCommand_RebuildsFTSAfterSanitizedReplay(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedFTSScenario(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.Equal(t, "1", queryDB(t, destPath, "SELECT count(*) FROM fts_nodes"))
}

func TestRecoverCommand_PreservesAndRestoresVectorProfileMetadata(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeVecBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	queryDB(t, dbPath, "INSERT INTO vector_profiles (profile, table_name, dimension, enabled) VALUES ('default', 'vec_nodes_active', 4, 1)")

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.Equal(t, "4", queryDB(t, destPath, "SELECT dimension FROM vector_profiles WHERE profile = 'default'"))
	require.Equal(t, "1", queryDB(t, destPath, "SELECT count(*) FROM sqlite_schema WHERE name = 'vec_nodes_active'"))
}

func TestRecoverCommand_LargeTruncationRecoversSomething(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	testutil.InjectLargeTruncation(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.FileExists(t, destPath)
}

func TestRecoverCommand_TruncatedDBHandlesZeroRows(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	testutil.InjectTruncation(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.FileExists(t, destPath)
}

func TestRecoverCommand_HeaderCorruptedDBHandlesZeroRows(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedTraceScenario(t, dbPath)
	testutil.InjectHeaderCorruption(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.FileExists(t, destPath)
}

func TestRecoverCommand_MissingDestDirIsCreated(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	// Nested subdir that does not yet exist.
	destPath := filepath.Join(tempDir, "subdir", "nested", "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.FileExists(t, destPath)
}

func TestRecoverCommand_DestAlreadyExistsIsRejected(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "existing.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	// Create dest file before running recover.
	require.NoError(t, os.WriteFile(destPath, []byte("placeholder"), 0o644))

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.Error(t, err)
	require.Contains(t, string(output), "already exists")
}

func TestRecoverCommand_MissingDBIsRejected(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	destPath := filepath.Join(tempDir, "recovered.db")

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", "/nonexistent-fathom-db.db",
		"--dest", destPath,
	)

	output, err := cmd.CombinedOutput()

	require.Error(t, err)
	require.Contains(t, string(output), "source database")
}

// buildCmd is a helper that constructs a `go run ./cmd/fathom-integrity` command.
func buildCmd(repoRoot string, args ...string) *exec.Cmd {
	allArgs := append([]string{"run", "./cmd/fathom-integrity"}, args...)
	cmd := exec.Command("go", allArgs...)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()
	return cmd
}
