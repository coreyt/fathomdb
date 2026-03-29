package e2e

import (
	"fmt"
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

func TestRecoverCommand_PreservesMultilineChunkTextWithSqlErrorPrefix(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedMultilineChunkScenario(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.Equal(
		t,
		"line 1\nsql error: preserved text inside chunk\nline 3",
		queryDB(t, destPath, "SELECT text_content FROM chunks WHERE id = 'chunk-1'"),
	)
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

func TestRecoverCommand_RegeneratesVectorEmbeddingsAndSupportsVectorSearch(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeVecBridgeScript(t, tempDir, repoRoot)
	configPath := filepath.Join(tempDir, "vector-regen.toml")
	generatorPath := filepath.Join(tempDir, "vector-generator.sh")

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedVectorRegenerationScenario(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.Equal(
		t,
		"1",
		queryDB(t, destPath, "SELECT count(*) FROM vector_embedding_contracts WHERE profile = 'default'"),
	)

	require.NoError(t, os.WriteFile(generatorPath, []byte(`#!/usr/bin/env bash
set -euo pipefail
python3 -c 'import json, sys
payload = json.load(sys.stdin)
embeddings = []
for chunk in payload["chunks"]:
    if "budget" in chunk["text_content"].lower():
        embedding = [1.0, 0.0, 0.0, 0.0]
    else:
        embedding = [0.0, 1.0, 0.0, 0.0]
    embeddings.append({"chunk_id": chunk["chunk_id"], "embedding": embedding})
json.dump({"embeddings": embeddings}, sys.stdout)'
`), 0o755))
	require.NoError(t, os.WriteFile(configPath, []byte(fmt.Sprintf(`
profile = "default"
table_name = "vec_nodes_active"
model_identity = "test-model"
model_version = "1.0.0"
dimension = 4
normalization_policy = "l2"
chunking_policy = "per_chunk"
preprocessing_policy = "trim"
generator_command = [%q]
`, generatorPath)), 0o644))

	cmd = buildCmd(repoRoot,
		"regenerate-vectors",
		"--db", destPath,
		"--bridge", bridgePath,
		"--config", configPath,
	)
	output, err = cmd.CombinedOutput()
	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "vector embeddings regenerated")
	require.Equal(t, "1", queryDB(t, destPath, "SELECT count(*) FROM vector_embedding_contracts WHERE profile = 'default'"))

	result := pythonVectorSearch(t, repoRoot, destPath)
	require.False(t, result.WasDegraded, "vector search must not degrade after regeneration")
	require.Contains(t, result.LogicalIDs, "doc-1")
}

func TestRecoverCommand_RegenerateVectorsRejectsConcurrentChunkChange(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeVecBridgeScript(t, tempDir, repoRoot)
	configPath := filepath.Join(tempDir, "vector-regen.toml")
	generatorPath := filepath.Join(tempDir, "vector-generator-drift.sh")

	bootstrapBridgeDB(t, bridgePath, dbPath)
	testutil.SeedVectorRegenerationScenario(t, dbPath)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))

	require.NoError(t, os.WriteFile(generatorPath, []byte(fmt.Sprintf(`#!/usr/bin/env bash
set -euo pipefail
python3 -c 'import json, sqlite3, sys
payload = json.load(sys.stdin)
conn = sqlite3.connect(%q)
conn.execute("INSERT INTO chunks (id, node_logical_id, text_content, created_at) VALUES (?, ?, ?, unixepoch())", ("chunk-2", "doc-1", "late arriving text"))
conn.commit()
conn.close()
embeddings = [{"chunk_id": chunk["chunk_id"], "embedding": [1.0, 0.0, 0.0, 0.0]} for chunk in payload["chunks"]]
json.dump({"embeddings": embeddings}, sys.stdout)'
`, destPath)), 0o755))
	require.NoError(t, os.WriteFile(configPath, []byte(fmt.Sprintf(`
profile = "default"
table_name = "vec_nodes_active"
model_identity = "test-model"
model_version = "1.0.0"
dimension = 4
normalization_policy = "l2"
chunking_policy = "per_chunk"
preprocessing_policy = "trim"
generator_command = [%q]
`, generatorPath)), 0o644))

	cmd = buildCmd(repoRoot,
		"regenerate-vectors",
		"--db", destPath,
		"--bridge", bridgePath,
		"--config", configPath,
	)
	output, err = cmd.CombinedOutput()
	require.Error(t, err, string(output))
	require.Contains(t, string(output), "regenerate vectors failed")
	require.Equal(t, "test-model", queryDB(t, destPath, "SELECT model_identity FROM vector_embedding_contracts WHERE profile = 'default'"))
	result := pythonVectorSearch(t, repoRoot, destPath)
	require.False(t, result.WasDegraded, "vector capability should still be present after failed regeneration")
	require.Empty(t, result.LogicalIDs)
}

func TestRecoverCommand_PreservesRetiredRestorableStateWithoutTreatingItAsCorruption(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	destPath := filepath.Join(tempDir, "recovered.db")
	bridgePath := makeBridgeScript(t, tempDir, repoRoot)

	bootstrapBridgeDB(t, bridgePath, dbPath)
	queryDB(t, dbPath, `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref)
VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 200, 'source-1');
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('chunk-1', 'doc-1', 'budget discussion', 100);
INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json)
VALUES ('evt-1', 'node_retire', 'doc-1', 'forget-1', 200, '');
`)

	cmd := buildCmd(repoRoot,
		"recover",
		"--db", dbPath,
		"--dest", destPath,
		"--bridge", bridgePath,
	)

	output, err := cmd.CombinedOutput()

	require.NoError(t, err, string(output))
	require.Contains(t, string(output), "recover completed")
	require.Equal(t, "1", queryDB(t, destPath, "SELECT count(*) FROM nodes WHERE logical_id = 'doc-1'"))
	require.Equal(t, "0", queryDB(t, destPath, "SELECT count(*) FROM nodes WHERE logical_id = 'doc-1' AND superseded_at IS NULL"))
	require.Equal(t, "1", queryDB(t, destPath, "SELECT count(*) FROM chunks WHERE node_logical_id = 'doc-1'"))
	require.Equal(t, "0", queryDB(t, destPath, "SELECT count(*) FROM fts_nodes WHERE node_logical_id = 'doc-1'"))
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
