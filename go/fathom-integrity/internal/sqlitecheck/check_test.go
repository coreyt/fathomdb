package sqlitecheck

import (
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

func TestDiagnoseDetectsWALPresence(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "waltest.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	// Place a WAL sentinel file.  SQLite ignores files with an invalid WAL header
	// and proceeds from the main database, so integrity_check still returns "ok".
	require.NoError(t, os.WriteFile(dbPath+"-wal", []byte("WAL"), 0o644))

	report, err := Diagnose(dbPath, sqliteBin, nil)

	require.NoError(t, err)
	require.True(t, report.Layer1.WALPresent)
	// WAL presence is informational, not an error.
	require.Equal(t, "clean", report.Overall)
}
