package testutil

import (
	"os"
	"os/exec"
	"testing"

	"github.com/stretchr/testify/require"
)

// SeedTraceScenario inserts a node, run, step, and action all tagged with
// source-1 into an already-bootstrapped database.  The database must exist
// and have the fathomdb schema applied before this helper is called.
func SeedTraceScenario(t *testing.T, dbPath string) {
	t.Helper()

	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
INSERT INTO runs (id, kind, status, properties, created_at, source_ref)
VALUES ('run-1', 'session', 'completed', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
INSERT INTO steps (id, run_id, kind, status, properties, created_at, source_ref)
VALUES ('step-1', 'run-1', 'llm', 'completed', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
INSERT INTO actions (id, step_id, kind, status, properties, created_at, source_ref)
VALUES ('action-1', 'step-1', 'emit', 'completed', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
`
	runSQLite(t, dbPath, sql)
}

// SeedExciseScenario inserts two versions of meeting-1: version 1 (source-1)
// already superseded, and version 2 (source-2) as the current active row.
// The database must be bootstrapped before this helper is called.
func SeedExciseScenario(t *testing.T, dbPath string) {
	t.Helper()

	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', 100, 200, 'source-1');
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-2', 'meeting-1', 'Meeting', '{}', 200, 'source-2');
`
	runSQLite(t, dbPath, sql)
}

// SeedFTSRepairScenario inserts a node and a chunk with an FTS row, then
// deletes the FTS row so that rebuild can restore it.
func SeedFTSRepairScenario(t *testing.T, dbPath string) {
	t.Helper()

	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('chunk-1', 'meeting-1', 'budget discussion', CAST(strftime('%s','now') AS INTEGER));
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('chunk-1', 'meeting-1', 'Meeting', 'budget discussion');
DELETE FROM fts_nodes;
`
	runSQLite(t, dbPath, sql)
}

// SeedFTSScenario inserts a node, chunk, and matching FTS row.
func SeedFTSScenario(t *testing.T, dbPath string) {
	t.Helper()

	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('chunk-1', 'meeting-1', 'budget discussion', CAST(strftime('%s','now') AS INTEGER));
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('chunk-1', 'meeting-1', 'Meeting', 'budget discussion');
`
	runSQLite(t, dbPath, sql)
}

// SeedMultilineChunkScenario inserts a node plus a chunk whose text contains a
// line that begins with "sql error:" so recover sanitization can prove it does
// not corrupt multiline content.
func SeedMultilineChunkScenario(t *testing.T, dbPath string) {
	t.Helper()

	sql := `
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-1', 'meeting-1', 'Meeting', '{}', CAST(strftime('%s','now') AS INTEGER), 'source-1');
INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('chunk-1', 'meeting-1', 'line 1
sql error: preserved text inside chunk
line 3', CAST(strftime('%s','now') AS INTEGER));
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('chunk-1', 'meeting-1', 'Meeting', 'line 1
sql error: preserved text inside chunk
line 3');
`
	runSQLite(t, dbPath, sql)
}

func runSQLite(t *testing.T, dbPath, sql string) {
	t.Helper()

	cmd := exec.Command(SQLiteBinary(), dbPath, sql) //nolint:gosec // G204: SQLiteBinary() returns controlled path from policy or hardcoded default
	cmd.Env = os.Environ()

	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))
}
