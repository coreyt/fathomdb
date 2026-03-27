package testutil

import (
	"encoding/binary"
	"os"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/walcheck"
	"github.com/stretchr/testify/require"
)

// InjectHeaderCorruption overwrites the SQLite magic header bytes, making the
// file unrecognizable as a valid SQLite database.
func InjectHeaderCorruption(t *testing.T, path string) {
	t.Helper()
	f, err := os.OpenFile(path, os.O_RDWR, 0o600)
	require.NoError(t, err)
	defer f.Close()
	_, err = f.WriteAt([]byte("NOT_SQLITE_DB!!!"), 0)
	require.NoError(t, err)
}

// InjectTruncation truncates the database file to 512 bytes, simulating a
// partial write failure or storage truncation mid-page.
func InjectTruncation(t *testing.T, path string) {
	t.Helper()
	require.NoError(t, os.Truncate(path, 512))
}

// InjectFTSDeletion removes all rows from fts_nodes, simulating a failed
// projection write or direct table corruption.
func InjectFTSDeletion(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, "DELETE FROM fts_nodes;")
}

// InjectNullSourceRef sets source_ref to NULL on all active nodes, simulating
// loss of provenance metadata.
func InjectNullSourceRef(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, "UPDATE nodes SET source_ref = NULL WHERE superseded_at IS NULL;")
}

// InjectOrphanedChunk inserts a chunk that references a non-existent node,
// simulating a partial write failure or missed FK constraint.
func InjectOrphanedChunk(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `INSERT INTO chunks (id, node_logical_id, text_content, created_at)
VALUES ('orphan-chunk-1', 'does-not-exist', 'orphaned content', unixepoch());`)
}

// InjectBrokenStepFK inserts a step that references a non-existent run_id,
// simulating a partial write failure that left an orphaned runtime table row.
// The sqlite3 CLI has FK enforcement off by default, so the insert succeeds.
func InjectBrokenStepFK(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `INSERT INTO steps (id, run_id, kind, status, properties, created_at)
VALUES ('ghost-step-1', 'ghost-run', 'llm', 'completed', '{}', unixepoch());`)
}

// InjectBrokenActionFK inserts an action that references a non-existent step_id,
// simulating a partial write failure that left a dangling action row.
func InjectBrokenActionFK(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `INSERT INTO actions (id, step_id, kind, status, properties, created_at)
VALUES ('ghost-action-1', 'ghost-step', 'emit', 'completed', '{}', unixepoch());`)
}

// InjectLargeTruncation truncates the database to 50% of its current size,
// preserving early pages (which are more likely to contain recoverable rows)
// while discarding the latter half.
func InjectLargeTruncation(t *testing.T, path string) {
	t.Helper()
	info, err := os.Stat(path)
	require.NoError(t, err)
	require.NoError(t, os.Truncate(path, info.Size()/2))
}

// InjectBrokenSupersession creates two active rows for the same logical_id by
// dropping the unique partial index before inserting the duplicate, simulating a
// failed transaction during upsert.
func InjectBrokenSupersession(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `DROP INDEX IF EXISTS idx_nodes_active_logical_id;
INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref)
VALUES ('row-dup', 'meeting-1', 'Meeting', '{}', unixepoch(), 'source-duplicate');`)
}

// InjectStaleFTSRow inserts an FTS row that references a non-existent chunk_id.
// Creates: stale_fts_rows detectable by check_semantics.
func InjectStaleFTSRow(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('ghost-chunk-id', 'meeting-1', 'Meeting', 'stale fts content');`)
}

// InjectFTSForSupersededNode adds an FTS row for a node that has been superseded
// (superseded_at IS NOT NULL). Creates: fts_rows_for_superseded_nodes.
func InjectFTSForSupersededNode(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `
UPDATE nodes SET superseded_at = datetime('now') WHERE logical_id = 'meeting-1' AND superseded_at IS NULL;
INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
VALUES ('chunk-superseded', 'meeting-1', 'Meeting', 'superseded node fts content');`)
}

// InjectPartialExcision partially excises a provenance chain: deletes the run
// record for source-1 but leaves its child steps intact.
// Creates: broken_step_fk chains detectable by check_semantics.
func InjectPartialExcision(t *testing.T, dbPath string) {
	t.Helper()
	runSQLite(t, dbPath, `DELETE FROM runs WHERE source_ref = 'source-1';`)
}

// InjectWALBitFlip flips a single byte in the page data of the given WAL frame
// (0-based frameIndex), at byteOffset within the page content area. This simulates
// a silent storage bit-flip that corrupts the frame checksum chain from that frame
// onward — the most dangerous known SQLite failure mode.
//
// The WAL file must already exist and contain at least frameIndex+1 complete frames.
// The page size is read from the WAL header.
func InjectWALBitFlip(t *testing.T, walPath string, frameIndex int, byteOffset int) {
	t.Helper()
	data, err := os.ReadFile(walPath)
	require.NoError(t, err)
	require.GreaterOrEqual(t, len(data), walcheck.WALHeaderSize, "WAL file too short to have a valid header")

	pageSize := int(binary.BigEndian.Uint32(data[8:12]))
	require.Greater(t, pageSize, 0, "WAL header has invalid page size")

	frameSize := walcheck.WALFrameHeaderSize + pageSize
	// Page data starts at walcheck.WALFrameHeaderSize within each frame.
	offset := walcheck.WALHeaderSize + frameIndex*frameSize + walcheck.WALFrameHeaderSize + byteOffset
	require.Less(t, offset, len(data), "bit flip offset %d is beyond WAL file size %d", offset, len(data))

	data[offset] ^= 0xFF
	require.NoError(t, os.WriteFile(walPath, data, 0o644))
}
