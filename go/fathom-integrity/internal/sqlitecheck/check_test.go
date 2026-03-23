package sqlitecheck

import (
	"encoding/binary"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

func TestRunValidatesSQLiteHeader(t *testing.T) {
	databasePath := filepath.Join(t.TempDir(), "fathom.db")
	content := append([]byte(sqliteHeader), make([]byte, 128)...)
	require.NoError(t, os.WriteFile(databasePath, content, 0o644))

	report, err := Run(databasePath)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Empty(t, report.Warnings)
}

func TestRunFlagsInvalidHeader(t *testing.T) {
	databasePath := filepath.Join(t.TempDir(), "not-sqlite.db")
	require.NoError(t, os.WriteFile(databasePath, []byte("not sqlite"), 0o644))

	report, err := Run(databasePath)

	require.Error(t, err)
	require.Equal(t, Report{}, report)
}

func TestDiagnoseCleanDB(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "test.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	report, err := Diagnose(dbPath, sqliteBin, nil)

	require.NoError(t, err)
	require.True(t, report.Layer1.HeaderValid)
	require.True(t, report.Layer1.IntegrityCheckOK)
	require.Equal(t, 0, report.Layer1.ForeignKeyViolations)
	require.Equal(t, "clean", report.Overall)
}

func TestDiagnoseDetectsHeaderCorruption(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "corrupt.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	testutil.InjectHeaderCorruption(t, dbPath)

	report, err := Diagnose(dbPath, sqliteBin, nil)

	require.NoError(t, err)
	require.False(t, report.Layer1.HeaderValid)
	require.Equal(t, "corrupted", report.Overall)
}

func TestCountTable_ReturnsZeroForEmptyTable(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "test.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE nodes (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	n, err := CountTable(sqliteBin, dbPath, "nodes")
	require.NoError(t, err)
	require.Equal(t, 0, n)
}

func TestCountTable_ReturnsZeroForMissingTable(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "test.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	n, err := CountTable(sqliteBin, dbPath, "nonexistent_table")
	require.NoError(t, err)
	require.Equal(t, 0, n)
}

func buildValidWALHeader(pageSize uint32) []byte {
	buf := make([]byte, 32)
	binary.BigEndian.PutUint32(buf[0:4], 0x377f0682) // WAL magic BE
	binary.BigEndian.PutUint32(buf[4:8], 3007000)
	binary.BigEndian.PutUint32(buf[8:12], pageSize)
	return buf
}

func TestDiagnoseDetectsWALPresence(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "waltest.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	// Write a valid 32-byte WAL header with no frames.
	// SQLite ignores files with no valid committed frames and proceeds from the main database.
	require.NoError(t, os.WriteFile(dbPath+"-wal", buildValidWALHeader(4096), 0o644))

	report, err := Diagnose(dbPath, sqliteBin, nil)

	require.NoError(t, err)
	require.True(t, report.Layer1.WALPresent)
	require.True(t, report.Layer1.WAL.HeaderValid)
	require.Equal(t, 0, report.Layer1.WAL.FrameCount)
	// Valid WAL header with no frames generates no findings.
	require.Equal(t, "clean", report.Overall)
}

func TestDiagnoseDetectsInvalidWALHeader(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "waltest.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	// A 3-byte file has no valid WAL header.
	require.NoError(t, os.WriteFile(dbPath+"-wal", []byte("WAL"), 0o644))

	report, err := Diagnose(dbPath, sqliteBin, nil)

	require.NoError(t, err)
	require.True(t, report.Layer1.WALPresent)
	require.False(t, report.Layer1.WAL.HeaderValid)
	require.Equal(t, "corrupted", report.Overall)
}
