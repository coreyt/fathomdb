package commands

import (
	"bytes"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

func TestRunRepairFixesDuplicateActiveLogicalIDs(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dbPath := filepath.Join(t.TempDir(), "repair-duplicate.db")
	bootstrapRepairTestDB(t, sqliteBin, dbPath)

	runSQLite(t, sqliteBin, dbPath, `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 'source-1');
DROP INDEX IF EXISTS idx_nodes_active_logical_id;
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-2', 'meeting-1', 'Meeting', '{}', 200, 'source-2');
`)

	var out bytes.Buffer
	err := RunRepair(dbPath, "", sqliteBin, RepairTargetDuplicateActive, false, &out)
	require.NoError(t, err, out.String())
	require.Contains(t, out.String(), "repair completed")

	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM nodes WHERE logical_id = 'meeting-1' AND superseded_at IS NULL"))
	require.Equal(t, "row-2", queryScalar(t, sqliteBin, dbPath, "SELECT row_id FROM nodes WHERE logical_id = 'meeting-1' AND superseded_at IS NULL"))
	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM sqlite_schema WHERE name = 'idx_nodes_active_logical_id'"))
	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM provenance_events WHERE event_type = 'repair_duplicate_active_node'"))
}

func TestRunRepairFixesBrokenRuntimeForeignKeys(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dbPath := filepath.Join(t.TempDir(), "repair-runtime.db")
	bootstrapRepairTestDB(t, sqliteBin, dbPath)

	runSQLite(t, sqliteBin, dbPath, `
INSERT INTO runs (id, kind, status, properties, created_at, source_ref)
VALUES ('run-1', 'session', 'completed', '{}', 100, 'source-1');
INSERT INTO steps (id, run_id, kind, status, properties, created_at, source_ref)
VALUES ('step-good', 'run-1', 'llm', 'completed', '{}', 100, 'source-1');
INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref)
VALUES ('action-good', 'step-good', 'emit', 'completed', '{}', 100, 'source-1');
INSERT INTO steps (id, run_id, kind, status, properties, created_at, source_ref)
VALUES ('step-bad', 'ghost-run', 'llm', 'completed', '{}', 100, 'source-bad');
INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref)
VALUES ('action-bad-step', 'ghost-step', 'emit', 'completed', '{}', 100, 'source-bad');
INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref)
VALUES ('action-bad-run', 'step-bad', 'emit', 'completed', '{}', 100, 'source-bad');
`)

	var out bytes.Buffer
	err := RunRepair(dbPath, "", sqliteBin, RepairTargetRuntimeFK, false, &out)
	require.NoError(t, err, out.String())
	require.Contains(t, out.String(), "repair completed")

	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM steps"))
	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM actions"))
	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM provenance_events WHERE event_type = 'repair_delete_broken_step'"))
	require.Equal(t, "2", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM provenance_events WHERE event_type = 'repair_delete_broken_action'"))
}

func TestRunRepairFixesOrphanedChunksAndProjectionRows(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dbPath := filepath.Join(t.TempDir(), "repair-orphaned.db")
	bootstrapRepairTestDB(t, sqliteBin, dbPath)

	runSQLite(t, sqliteBin, dbPath, `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 'source-1');
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('chunk-1', 'meeting-1', 'good content', 100);
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('chunk-1', 'meeting-1', 'Meeting', 'good content');
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('orphan-1', 'ghost-node', 'orphaned content', 100);
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('orphan-1', 'ghost-node', 'Meeting', 'orphaned content');
`)

	var out bytes.Buffer
	err := RunRepair(dbPath, "", sqliteBin, RepairTargetOrphanedChunks, false, &out)
	require.NoError(t, err, out.String())
	require.Contains(t, out.String(), "repair completed")

	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM chunks"))
	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM fts_nodes"))
	require.Equal(t, "0", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM chunks WHERE id = 'orphan-1'"))
	require.Equal(t, "1", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM provenance_events WHERE event_type = 'repair_delete_orphaned_chunk'"))
}

func TestRunRepairDryRunLeavesDatabaseUnchanged(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dbPath := filepath.Join(t.TempDir(), "repair-dry-run.db")
	bootstrapRepairTestDB(t, sqliteBin, dbPath)

	runSQLite(t, sqliteBin, dbPath, `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 'source-1');
DROP INDEX IF EXISTS idx_nodes_active_logical_id;
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-2', 'meeting-1', 'Meeting', '{}', 200, 'source-2');
`)

	var out bytes.Buffer
	err := RunRepair(dbPath, "", sqliteBin, RepairTargetDuplicateActive, true, &out)
	require.NoError(t, err, out.String())

	require.Equal(t, "2", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM nodes WHERE logical_id = 'meeting-1' AND superseded_at IS NULL"))
	require.Equal(t, "0", queryScalar(t, sqliteBin, dbPath, "SELECT count(*) FROM provenance_events"))
}

func bootstrapRepairTestDB(t *testing.T, sqliteBin, dbPath string) {
	t.Helper()
	runSQLite(t, sqliteBin, dbPath, `
PRAGMA foreign_keys = OFF;
CREATE TABLE IF NOT EXISTS nodes (
    row_id TEXT PRIMARY KEY,
    logical_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    properties BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    superseded_at INTEGER,
    source_ref TEXT,
    confidence REAL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_active_logical_id
    ON nodes(logical_id)
    WHERE superseded_at IS NULL;
CREATE TABLE IF NOT EXISTS chunks (
    id TEXT PRIMARY KEY,
    node_logical_id TEXT NOT NULL,
    text_content TEXT NOT NULL,
    byte_start INTEGER,
    byte_end INTEGER,
    created_at INTEGER NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS fts_nodes USING fts5(
    chunk_id UNINDEXED,
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    properties BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    superseded_at INTEGER,
    source_ref TEXT
);
CREATE TABLE IF NOT EXISTS steps (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    properties BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    superseded_at INTEGER,
    source_ref TEXT
);
CREATE TABLE IF NOT EXISTS actions (
    id TEXT PRIMARY KEY,
    step_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    properties BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    superseded_at INTEGER,
    source_ref TEXT
);
CREATE TABLE IF NOT EXISTS provenance_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    subject TEXT NOT NULL,
    source_ref TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
`)
}

func runSQLite(t *testing.T, sqliteBin, dbPath, sql string) {
	t.Helper()
	cmd := exec.Command(sqliteBin, dbPath, sql)
	cmd.Env = os.Environ()
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
}

func queryScalar(t *testing.T, sqliteBin, dbPath, sql string) string {
	t.Helper()
	cmd := exec.Command(sqliteBin, dbPath, sql)
	cmd.Env = os.Environ()
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
	return string(bytes.TrimSpace(output))
}
