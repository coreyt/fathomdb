package sqlitecheck

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/walcheck"
)

const sqliteHeader = "SQLite format 3\x00"

// --- Legacy single-layer report (kept for backward compatibility) ---

type Report struct {
	Path        string
	HeaderValid bool
	SizeBytes   int64
	Warnings    []string
}

func Run(path string) (Report, error) {
	file, err := os.Open(path)
	if err != nil {
		return Report{}, err
	}
	defer file.Close()

	header := make([]byte, len(sqliteHeader))
	if _, err := io.ReadFull(file, header); err != nil {
		return Report{}, err
	}

	info, err := file.Stat()
	if err != nil {
		return Report{}, err
	}

	report := Report{
		Path:        path,
		HeaderValid: string(header) == sqliteHeader,
		SizeBytes:   info.Size(),
	}
	if !report.HeaderValid {
		report.Warnings = append(report.Warnings, "file header does not match SQLite format 3")
	}
	if report.SizeBytes < 100 {
		report.Warnings = append(report.Warnings, "database file is unusually small")
	}
	return report, nil
}

func Format(report Report) string {
	return fmt.Sprintf(
		"path=%s header_valid=%t size_bytes=%d warnings=%d",
		report.Path,
		report.HeaderValid,
		report.SizeBytes,
		len(report.Warnings),
	)
}

// --- Layered diagnostic report ---

type Finding struct {
	Layer    int    `json:"layer"`
	Severity string `json:"severity"` // "warning", "error", "critical"
	Message  string `json:"message"`
}

type Layer1Report struct {
	HeaderValid          bool               `json:"header_valid"`
	WALPresent           bool               `json:"wal_present"`
	WAL                  walcheck.WALReport `json:"wal"`
	IntegrityCheckOK     bool               `json:"integrity_check_ok"`
	IntegrityCheckDetail string             `json:"integrity_check_detail"`
	ForeignKeyViolations int                `json:"foreign_key_violations"`
	Findings             []Finding          `json:"findings"`
}

type Layer2Report struct {
	Available                 bool      `json:"available"`
	PhysicalOK                bool      `json:"physical_ok"`
	ForeignKeysOK             bool      `json:"foreign_keys_ok"`
	MissingFTSRows            int       `json:"missing_fts_rows"`
	DuplicateActiveLogicalIDs int       `json:"duplicate_active_logical_ids"`
	BrokenStepFK              int       `json:"broken_step_fk"`
	BrokenActionFK            int       `json:"broken_action_fk"`
	Findings                  []Finding `json:"findings"`
}

type Layer3Report struct {
	StaleFTSRows       int       `json:"stale_fts_rows"`
	OrphanedChunks     int       `json:"orphaned_chunks"`
	NullSourceRefNodes int       `json:"null_source_ref_nodes"`
	Findings           []Finding `json:"findings"`
}

type DiagnosticReport struct {
	DatabasePath string       `json:"database_path"`
	CheckedAt    string       `json:"checked_at"`
	Layer1       Layer1Report `json:"layer1"`
	Layer2       Layer2Report `json:"layer2"`
	Layer3       Layer3Report `json:"layer3"`
	Overall      string       `json:"overall"` // "clean", "degraded", "corrupted"
	Suggestions  []string     `json:"suggestions"`
}

// Diagnose runs a layered diagnostic against a SQLite database.
// sqliteBin is the path to the sqlite3 binary; pass "" to use exec.LookPath("sqlite3").
// layer2 is an optional pre-fetched Layer2Report from the admin bridge; pass nil to skip.
func Diagnose(dbPath, sqliteBin string, layer2 *Layer2Report) (DiagnosticReport, error) {
	if sqliteBin == "" {
		bin, err := exec.LookPath("sqlite3")
		if err != nil {
			return DiagnosticReport{}, fmt.Errorf("sqlite3 not found in PATH: %w", err)
		}
		sqliteBin = bin
	}

	report := DiagnosticReport{
		DatabasePath: dbPath,
		CheckedAt:    time.Now().UTC().Format(time.RFC3339),
		Suggestions:  []string{},
	}

	layer1, err := diagnoseLayer1(dbPath, sqliteBin)
	if err != nil {
		return DiagnosticReport{}, err
	}
	report.Layer1 = layer1

	if layer2 != nil {
		report.Layer2 = *layer2
	} else {
		report.Layer2 = Layer2Report{Findings: []Finding{}}
	}

	// Layer 3 only makes sense if we can open the file.
	if layer1.HeaderValid {
		report.Layer3 = diagnoseLayer3(dbPath, sqliteBin)
	} else {
		report.Layer3 = Layer3Report{Findings: []Finding{}}
	}

	report.Overall = computeOverall(report)
	report.Suggestions = computeSuggestions(report)
	return report, nil
}

// validTableNames is the allowlist of table names that CountTable accepts.
// Security fix H-2: Prevents SQL injection via table name concatenation by
// rejecting any table name not in this set.
var validTableNames = map[string]bool{
	"nodes": true, "chunks": true, "runs": true, "steps": true, "actions": true,
	"edges": true, "fts_nodes": true,
}

// CountTable returns the number of rows in the named table.
// Security fix M-3: Returns the error instead of silently returning (0, nil)
// so callers can distinguish "table missing" from "zero rows".
func CountTable(sqliteBin, dbPath, table string) (int, error) {
	// Security fix H-2: Validate table name against an allowlist to prevent
	// SQL injection. The table parameter is interpolated into the query string
	// and cannot be parameterized in sqlite3 CLI invocations.
	if !validTableNames[table] {
		return 0, fmt.Errorf("CountTable: invalid table name %q", table)
	}
	out, err := runSQLiteQuery(sqliteBin, dbPath, "SELECT count(*) FROM "+table+";")
	if err != nil {
		return 0, fmt.Errorf("CountTable(%q): %w", table, err)
	}
	n, err := strconv.Atoi(strings.TrimSpace(out))
	if err != nil {
		return 0, fmt.Errorf("parse count for table %q: %w", table, err)
	}
	return n, nil
}

// FormatDiagnostic serialises the report as a compact JSON string.
func FormatDiagnostic(r DiagnosticReport) (string, error) {
	b, err := json.Marshal(r)
	if err != nil {
		return "", err
	}
	return string(b), nil
}

func diagnoseLayer1(dbPath, sqliteBin string) (Layer1Report, error) {
	report := Layer1Report{Findings: []Finding{}}

	// 1. Read and validate the SQLite header (no subprocess needed).
	f, err := os.Open(dbPath)
	if err != nil {
		return report, err
	}
	hdr := make([]byte, len(sqliteHeader))
	_, err = io.ReadFull(f, hdr)
	f.Close()
	if err != nil {
		return report, err
	}

	report.HeaderValid = string(hdr) == sqliteHeader
	if !report.HeaderValid {
		report.Findings = append(report.Findings, Finding{
			Layer:    1,
			Severity: "critical",
			Message:  "file header does not match SQLite format 3",
		})
		// Cannot run PRAGMA queries on a non-SQLite file.
		return report, nil
	}

	// 2. Inspect WAL file (no subprocess needed).
	walReport, _ := walcheck.InspectWAL(dbPath + "-wal")
	report.WAL = walReport
	report.WALPresent = walReport.Present
	if walReport.Present {
		if !walReport.HeaderValid {
			report.Findings = append(report.Findings, Finding{
				Layer: 1, Severity: "error", Message: "WAL file has invalid header",
			})
		} else {
			if walReport.Truncated {
				report.Findings = append(report.Findings, Finding{
					Layer: 1, Severity: "warning",
					Message: fmt.Sprintf("WAL file truncated at byte %d", walReport.TruncationOffset),
				})
			}
			if walReport.CheckpointNeeded {
				report.Findings = append(report.Findings, Finding{
					Layer: 1, Severity: "warning",
					Message: fmt.Sprintf("WAL has %d frames; run PRAGMA wal_checkpoint(TRUNCATE)", walReport.FrameCount),
				})
			}
		}
	}

	// 3. PRAGMA integrity_check
	out, err := runSQLiteQuery(sqliteBin, dbPath, "PRAGMA integrity_check;")
	if err == nil {
		trimmed := strings.TrimSpace(out)
		if trimmed == "ok" {
			report.IntegrityCheckOK = true
			report.IntegrityCheckDetail = "ok"
		} else {
			report.IntegrityCheckOK = false
			report.IntegrityCheckDetail = trimmed
			report.Findings = append(report.Findings, Finding{
				Layer:    1,
				Severity: "error",
				Message:  "integrity_check: " + firstLine(trimmed),
			})
		}
	}

	// 4. PRAGMA foreign_key_check
	out, err = runSQLiteQuery(sqliteBin, dbPath, "PRAGMA foreign_key_check;")
	if err == nil {
		lines := nonEmptyLines(out)
		report.ForeignKeyViolations = len(lines)
		if len(lines) > 0 {
			report.Findings = append(report.Findings, Finding{
				Layer:    1,
				Severity: "error",
				Message:  fmt.Sprintf("%d foreign key violation(s)", len(lines)),
			})
		}
	}

	return report, nil
}

func diagnoseLayer3(dbPath, sqliteBin string) Layer3Report {
	report := Layer3Report{Findings: []Finding{}}

	// Stale FTS: chunks with an active node but no matching fts_nodes row.
	if n, ok := runSQLiteCount(sqliteBin, dbPath,
		"SELECT count(*) FROM chunks c "+
			"JOIN nodes n ON n.logical_id = c.node_logical_id AND n.superseded_at IS NULL "+
			"WHERE NOT EXISTS (SELECT 1 FROM fts_nodes f WHERE f.chunk_id = c.id)"); ok {
		report.StaleFTSRows = n
		if n > 0 {
			report.Findings = append(report.Findings, Finding{
				Layer: 3, Severity: "warning",
				Message: fmt.Sprintf("%d chunk(s) missing from fts_nodes", n),
			})
		}
	}

	// Orphaned chunks: chunks whose node_logical_id has no active node.
	if n, ok := runSQLiteCount(sqliteBin, dbPath,
		"SELECT count(*) FROM chunks c "+
			"WHERE NOT EXISTS (SELECT 1 FROM nodes n "+
			"WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL)"); ok {
		report.OrphanedChunks = n
		if n > 0 {
			report.Findings = append(report.Findings, Finding{
				Layer: 3, Severity: "warning",
				Message: fmt.Sprintf("%d orphaned chunk(s) with no active node", n),
			})
		}
	}

	// NULL source_ref on active nodes.
	if n, ok := runSQLiteCount(sqliteBin, dbPath,
		"SELECT count(*) FROM nodes WHERE source_ref IS NULL AND superseded_at IS NULL"); ok {
		report.NullSourceRefNodes = n
		if n > 0 {
			report.Findings = append(report.Findings, Finding{
				Layer: 3, Severity: "warning",
				Message: fmt.Sprintf("%d active node(s) with null source_ref", n),
			})
		}
	}

	return report
}

func computeOverall(r DiagnosticReport) string {
	for _, f := range r.Layer1.Findings {
		if f.Severity == "critical" || f.Severity == "error" {
			return "corrupted"
		}
	}
	for _, f := range r.Layer2.Findings {
		if f.Severity == "critical" || f.Severity == "error" {
			return "corrupted"
		}
	}
	all := append(append(r.Layer1.Findings, r.Layer2.Findings...), r.Layer3.Findings...) //nolint:gocritic
	for _, f := range all {
		if f.Severity == "warning" {
			return "degraded"
		}
	}
	return "clean"
}

func computeSuggestions(r DiagnosticReport) []string {
	var suggestions []string
	emitted := map[string]bool{}
	add := func(s string) {
		if !emitted[s] {
			emitted[s] = true
			suggestions = append(suggestions, s)
		}
	}

	// Layer 1 — storage-level suggestions.
	// Security fix M-5: All database paths in suggestions are now single-quoted
	// to prevent shell metacharacter injection when users copy-paste commands.
	if !r.Layer1.HeaderValid {
		add(fmt.Sprintf("database header is corrupt; attempt recovery with: fathom-integrity recover --db '%s' --dest <recovery-path>", r.DatabasePath))
	} else if !r.Layer1.IntegrityCheckOK {
		add(fmt.Sprintf("structural corruption detected; attempt recovery with: fathom-integrity recover --db '%s' --dest <recovery-path>", r.DatabasePath))
	}
	if r.Layer1.ForeignKeyViolations > 0 {
		add("foreign key violations present; investigate with: PRAGMA foreign_key_check;")
	}
	if r.Layer1.WAL.Present {
		if !r.Layer1.WAL.HeaderValid {
			add(fmt.Sprintf("WAL file has invalid header; remove if no writers are active: rm '%s-wal'", r.DatabasePath))
		} else {
			if r.Layer1.WAL.Truncated {
				add("WAL file is truncated; ensure no writers are active before proceeding")
			}
			if r.Layer1.WAL.CheckpointNeeded {
				add(fmt.Sprintf("WAL has %d frames; reclaim space with: sqlite3 '%s' 'PRAGMA wal_checkpoint(TRUNCATE);'", r.Layer1.WAL.FrameCount, r.DatabasePath))
			}
		}
	}

	// Layer 2 — engine-invariant suggestions.
	needsFTSRebuild := false
	if r.Layer2.Available {
		if r.Layer2.MissingFTSRows > 0 {
			needsFTSRebuild = true
		}
		if r.Layer2.DuplicateActiveLogicalIDs > 0 {
			add(fmt.Sprintf(
				"repair duplicate active logical_ids with: fathom-integrity repair --target duplicate-active --db '%s'",
				r.DatabasePath,
			))
		}
		if r.Layer2.BrokenStepFK > 0 || r.Layer2.BrokenActionFK > 0 {
			add(fmt.Sprintf(
				"repair broken runtime FK chains with: fathom-integrity repair --target runtime-fk --db '%s'",
				r.DatabasePath,
			))
		}
	}

	// Layer 3 — application-semantic suggestions.
	if r.Layer3.StaleFTSRows > 0 {
		needsFTSRebuild = true
	}
	if needsFTSRebuild {
		add(fmt.Sprintf("repair FTS projections with: fathom-integrity rebuild --target fts --db '%s'", r.DatabasePath))
	}
	if r.Layer3.OrphanedChunks > 0 {
		add(fmt.Sprintf(
			"repair orphaned chunks with: fathom-integrity repair --target orphaned-chunks --db '%s'",
			r.DatabasePath,
		))
	}
	if r.Layer3.NullSourceRefNodes > 0 {
		add("nodes missing source_ref cannot be excised by source; consider re-ingesting affected data")
	}

	return suggestions
}

func runSQLiteQuery(sqliteBin, dbPath, query string) (string, error) {
	cmd := exec.Command(sqliteBin, dbPath, query)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("%w: %s", err, strings.TrimSpace(string(out)))
	}
	return string(out), nil
}

func runSQLiteCount(sqliteBin, dbPath, query string) (int, bool) {
	out, err := runSQLiteQuery(sqliteBin, dbPath, query)
	if err != nil {
		return 0, false
	}
	n, err := strconv.Atoi(strings.TrimSpace(out))
	if err != nil {
		return 0, false
	}
	return n, true
}

func nonEmptyLines(s string) []string {
	var lines []string
	for _, line := range strings.Split(s, "\n") {
		if strings.TrimSpace(line) != "" {
			lines = append(lines, line)
		}
	}
	return lines
}

func firstLine(s string) string {
	if idx := strings.IndexByte(s, '\n'); idx >= 0 {
		return s[:idx]
	}
	return s
}
