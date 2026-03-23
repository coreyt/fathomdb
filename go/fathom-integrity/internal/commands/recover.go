package commands

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
)

// RecoverRowCounts holds the count of recovered rows for each key table.
type RecoverRowCounts struct {
	Nodes   int `json:"nodes"`
	Chunks  int `json:"chunks"`
	Runs    int `json:"runs"`
	Steps   int `json:"steps"`
	Actions int `json:"actions"`
}

// RecoverReport is the structured output of a recover operation.
type RecoverReport struct {
	SourceDB    string                       `json:"source_db"`
	RecoveredDB string                       `json:"recovered_db"`
	RowCounts   RecoverRowCounts             `json:"row_counts"`
	CheckResult sqlitecheck.DiagnosticReport `json:"check_result"`
}

// RunRecover runs sqlite3 .recover against sourcePath, replays the recovered SQL
// into destPath, bootstraps the fathomdb schema via the bridge, counts rows in
// key tables, runs a full layered diagnostic, and writes a JSON RecoverReport to out.
//
// Pass sqliteBin="" to use exec.LookPath("sqlite3").
// Pass bridgePath="" to skip bootstrap and Layer 2 checks.
func RunRecover(sourcePath, destPath, bridgePath, sqliteBin string, out io.Writer) error {
	if sqliteBin == "" {
		bin, err := exec.LookPath("sqlite3")
		if err != nil {
			return fmt.Errorf("sqlite3 not found in PATH: %w", err)
		}
		sqliteBin = bin
	}

	if _, err := os.Stat(sourcePath); err != nil {
		return fmt.Errorf("source database does not exist: %s", sourcePath)
	}

	if _, err := os.Stat(destPath); err == nil {
		return fmt.Errorf("destination already exists: %s — remove it first or choose a different path", destPath)
	}

	if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
		return fmt.Errorf("create dest directory: %w", err)
	}

	// Run sqlite3 .recover against the (possibly corrupt) source.
	// Non-zero exit is normal when the source is corrupt; we proceed as long as
	// some SQL was emitted.
	var recoveredSQL bytes.Buffer
	recoverCmd := exec.Command(sqliteBin, sourcePath, ".recover")
	recoverCmd.Stdout = &recoveredSQL
	recoverCmd.Stderr = io.Discard
	_ = recoverCmd.Run()

	// Replay the recovered SQL into the destination database.
	if recoveredSQL.Len() > 0 {
		replayCmd := exec.Command(sqliteBin, destPath)
		replayCmd.Stdin = &recoveredSQL
		var replayStderr bytes.Buffer
		replayCmd.Stderr = &replayStderr
		if err := replayCmd.Run(); err != nil {
			_ = os.Remove(destPath)
			return fmt.Errorf("replay .recover SQL: %w: %s", err, replayStderr.String())
		}
	}

	// If dest still doesn't exist (nothing was recovered and bridge will create it
	// via OPEN_CREATE), create an empty SQLite file so Diagnose always has a target.
	if _, err := os.Stat(destPath); os.IsNotExist(err) {
		if err := exec.Command(sqliteBin, destPath, "SELECT 1;").Run(); err != nil {
			return fmt.Errorf("create empty recovery target: %w", err)
		}
	}

	// Bootstrap the fathomdb schema and gather Layer 2 data via the bridge.
	// fetchLayer2 calls both check_integrity and check_semantics; check_integrity
	// calls schema_manager.bootstrap() which creates any schema objects missing
	// from the recovered database (e.g. the fts_nodes virtual table).
	var layer2 *sqlitecheck.Layer2Report
	if bridgePath != "" {
		l2, err := fetchLayer2(destPath, bridgePath)
		if err != nil {
			return fmt.Errorf("bootstrap and layer2 check: %w", err)
		}
		layer2 = &l2
	}

	// Count rows in the key fathomdb tables.
	rowCounts := RecoverRowCounts{}
	for _, pair := range []struct {
		field *int
		table string
	}{
		{&rowCounts.Nodes, "nodes"},
		{&rowCounts.Chunks, "chunks"},
		{&rowCounts.Runs, "runs"},
		{&rowCounts.Steps, "steps"},
		{&rowCounts.Actions, "actions"},
	} {
		n, _ := sqlitecheck.CountTable(sqliteBin, destPath, pair.table)
		*pair.field = n
	}

	// Run a full three-layer diagnostic on the recovered database.
	checkResult, err := sqlitecheck.Diagnose(destPath, sqliteBin, layer2)
	if err != nil {
		return fmt.Errorf("post-recovery check: %w", err)
	}

	report := RecoverReport{
		SourceDB:    sourcePath,
		RecoveredDB: destPath,
		RowCounts:   rowCounts,
		CheckResult: checkResult,
	}

	b, err := json.Marshal(report)
	if err != nil {
		return fmt.Errorf("marshal recover report: %w", err)
	}
	fmt.Fprintln(out, string(b))
	fmt.Fprintln(out, "recover completed")
	return nil
}
