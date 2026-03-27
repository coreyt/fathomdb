package sqlitecheck

import (
	"encoding/binary"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/walcheck"
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

func TestCountTable_RejectsUnknownTable(t *testing.T) {
	// Security fix H-2: CountTable now rejects table names not in the
	// allowlist rather than concatenating them into SQL.
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "test.db")

	cmd := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);")
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))

	_, err = CountTable(sqliteBin, dbPath, "nonexistent_table")
	require.Error(t, err)
	require.Contains(t, err.Error(), "invalid table name")
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

// --- repair_suggestions tests ---

func TestComputeSuggestions_CleanReport_NoSuggestions(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, PhysicalOK: true, ForeignKeysOK: true, Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.Empty(t, suggestions)
}

func TestComputeSuggestions_HeaderInvalid_SuggestsRecover(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: false, Findings: []Finding{}},
		Layer2:       Layer2Report{Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.Len(t, suggestions, 1)
	require.Contains(t, suggestions[0], "recover")
	require.Contains(t, suggestions[0], "/tmp/test.db")
}

func TestComputeSuggestions_IntegrityCheckFail_SuggestsRecover(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: false, Findings: []Finding{}},
		Layer2:       Layer2Report{Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, suggestions[0], "recover")
}

func TestComputeSuggestions_FKViolations_SuggestsCheck(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, ForeignKeyViolations: 3, Findings: []Finding{}},
		Layer2:       Layer2Report{Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, suggestions[0], "foreign_key_check")
}

func TestComputeSuggestions_WALCheckpointNeeded_SuggestsCheckpoint(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1: Layer1Report{
			HeaderValid:      true,
			IntegrityCheckOK: true,
			WAL:              walcheck.WALReport{Present: true, HeaderValid: true, FrameCount: 150, CheckpointNeeded: true},
			Findings:         []Finding{},
		},
		Layer2: Layer2Report{Findings: []Finding{}},
		Layer3: Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	found := false
	for _, s := range suggestions {
		if strings.Contains(s, "wal_checkpoint") {
			found = true
		}
	}
	require.True(t, found, "expected wal_checkpoint suggestion, got: %v", suggestions)
}

func TestComputeSuggestions_Layer2MissingFTS_SuggestsRebuild(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, MissingFTSRows: 5, Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	found := false
	for _, s := range suggestions {
		if strings.Contains(s, "rebuild") && strings.Contains(s, "fts") {
			found = true
		}
	}
	require.True(t, found, "expected rebuild --target fts suggestion, got: %v", suggestions)
}

func TestComputeSuggestions_Layer3StaleFTS_SuggestsRebuild(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, Findings: []Finding{}},
		Layer3:       Layer3Report{StaleFTSRows: 2, Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	found := false
	for _, s := range suggestions {
		if strings.Contains(s, "rebuild") && strings.Contains(s, "fts") {
			found = true
		}
	}
	require.True(t, found, "expected rebuild --target fts suggestion, got: %v", suggestions)
}

func TestComputeSuggestions_NoDuplicateRebuildWhenBothLayer2And3HaveFTS(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, MissingFTSRows: 3, Findings: []Finding{}},
		Layer3:       Layer3Report{StaleFTSRows: 2, Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	rebuildCount := 0
	for _, s := range suggestions {
		if strings.Contains(s, "rebuild") && strings.Contains(s, "fts") {
			rebuildCount++
		}
	}
	require.Equal(t, 1, rebuildCount, "expected exactly one rebuild fts suggestion")
}

func TestComputeSuggestions_OrphanedChunks_SuggestsRepair(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, Findings: []Finding{}},
		Layer3:       Layer3Report{OrphanedChunks: 1, Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, strings.Join(suggestions, "\n"), "repair --target orphaned-chunks")
}

func TestComputeSuggestions_DuplicateActiveSuggestsRepair(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, DuplicateActiveLogicalIDs: 1, Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, strings.Join(suggestions, "\n"), "repair --target duplicate-active")
}

func TestComputeSuggestions_BrokenRuntimeFkSuggestsRepair(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, BrokenStepFK: 1, Findings: []Finding{}},
		Layer3:       Layer3Report{Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, strings.Join(suggestions, "\n"), "repair --target runtime-fk")
}

func TestComputeSuggestions_OrphanedChunksSuggestsTargetedRepair(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, Findings: []Finding{}},
		Layer3:       Layer3Report{OrphanedChunks: 1, Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, strings.Join(suggestions, "\n"), "repair --target orphaned-chunks")
}

func TestComputeSuggestions_NullSourceRef_SuggestsReingest(t *testing.T) {
	r := DiagnosticReport{
		DatabasePath: "/tmp/test.db",
		Layer1:       Layer1Report{HeaderValid: true, IntegrityCheckOK: true, Findings: []Finding{}},
		Layer2:       Layer2Report{Available: true, Findings: []Finding{}},
		Layer3:       Layer3Report{NullSourceRefNodes: 4, Findings: []Finding{}},
	}

	suggestions := computeSuggestions(r)

	require.NotEmpty(t, suggestions)
	require.Contains(t, suggestions[0], "source_ref")
}

func TestDiagnoseCleanDB_SuggestionsIsEmpty(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "test.db")

	out, err := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);").CombinedOutput()
	require.NoError(t, err, string(out))

	report, err := Diagnose(dbPath, sqliteBin, nil)

	require.NoError(t, err)
	require.Empty(t, report.Suggestions)
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
