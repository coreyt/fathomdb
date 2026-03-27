package commands

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"unicode"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
)

type bridgeExecuteFunc func(context.Context, bridge.Request) (bridge.Response, error)

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
	return RunRecoverWithFeedback(
		sourcePath,
		destPath,
		bridgePath,
		sqliteBin,
		out,
		nil,
		bridge.FeedbackConfig{},
	)
}

func RunRecoverWithFeedback(
	sourcePath, destPath, bridgePath, sqliteBin string,
	out io.Writer,
	observer bridge.Observer,
	config bridge.FeedbackConfig,
) error {
	_, err := bridge.RunWithFeedback(
		context.Background(),
		"go",
		"recover",
		map[string]string{
			"source_path": sourcePath,
			"dest_path":   destPath,
		},
		observer,
		config,
		func(context.Context) (struct{}, error) {
			return struct{}{}, runRecover(sourcePath, destPath, bridgePath, sqliteBin, out)
		},
	)
	return err
}

func runRecover(sourcePath, destPath, bridgePath, sqliteBin string, out io.Writer) error {
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

	// Security fix M-2: Use O_CREATE|O_EXCL for atomic creation to eliminate
	// the TOCTOU race between Stat() and file creation. If the file already
	// exists, OpenFile returns an error atomically.
	// Security fix M-1: Use 0o700 for directories, 0o600 for files.
	if err := os.MkdirAll(filepath.Dir(destPath), 0o700); err != nil {
		return fmt.Errorf("create dest directory: %w", err)
	}
	destGuard, err := os.OpenFile(destPath, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return fmt.Errorf("destination already exists or cannot be created: %w", err)
	}
	destGuard.Close()
	// Remove the guard file so sqlite3 can create its own database file at
	// this path. The O_EXCL open above guarantees no other process created
	// the file between our check and this point.
	os.Remove(destPath)

	// Run sqlite3 .recover against the (possibly corrupt) source.
	// Non-zero exit is normal when the source is corrupt; we proceed as long as
	// some SQL was emitted.
	var recoveredSQL bytes.Buffer
	recoverCmd := exec.Command(sqliteBin, sourcePath, ".recover")
	recoverCmd.Stdout = &recoveredSQL
	recoverCmd.Stderr = io.Discard
	_ = recoverCmd.Run()

	// Replay the recovered SQL into the destination database.
	sanitizedSQL := sanitizeRecoveredSQL(recoveredSQL.String())
	if strings.TrimSpace(sanitizedSQL) != "" {
		replayCmd := exec.Command(sqliteBin, destPath)
		replayCmd.Stdin = bytes.NewBufferString(sanitizedSQL)
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
		// Recovery deliberately strips rebuildable projection schema from sqlite3
		// .recover output. Reset migration history so bridge bootstrap reapplies
		// the current fathomdb schema idempotently before Layer 2 checks run.
		resetMigrationsCmd := exec.Command(
			sqliteBin,
			destPath,
			"DROP TABLE IF EXISTS fathom_schema_migrations;",
		)
		if output, err := resetMigrationsCmd.CombinedOutput(); err != nil {
			return fmt.Errorf("reset recovered migration history: %w: %s", err, output)
		}

		l2, err := fetchLayer2(destPath, bridgePath)
		if err != nil {
			return fmt.Errorf("bootstrap and layer2 check: %w", err)
		}

		if err := restoreRecoveredProjections(destPath, bridgePath, sqliteBin); err != nil {
			return fmt.Errorf("restore recovered projections: %w", err)
		}

		l2, err = fetchLayer2(destPath, bridgePath)
		if err != nil {
			return fmt.Errorf("layer2 check after projection restore: %w", err)
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

func sanitizeRecoveredSQL(sql string) string {
	statements := splitRecoveredStatements(sql)
	kept := make([]string, 0, len(statements))
	for _, stmt := range statements {
		if shouldSkipRecoveredStatement(stmt) {
			continue
		}
		kept = append(kept, stmt)
	}
	if len(kept) == 0 {
		return ""
	}
	return strings.Join(kept, "\n") + "\n"
}

func restoreRecoveredProjections(destPath, bridgePath, sqliteBin string) error {
	client := bridge.Client{BinaryPath: bridgePath}

	if enabledProfiles, err := queryScalarInt(
		sqliteBin,
		destPath,
		"SELECT count(*) FROM vector_profiles WHERE enabled = 1;",
	); err != nil {
		return fmt.Errorf("count enabled vector profiles: %w", err)
	} else if enabledProfiles > 0 {
		if err := runBridgeCommand(client, destPath, bridge.CommandRestoreVector); err != nil {
			return fmt.Errorf("restore vector profiles: %w", err)
		}
	}

	if err := runBridgeCommand(client, destPath, bridge.CommandRebuildMissing); err != nil {
		return fmt.Errorf("rebuild missing projections: %w", err)
	}

	return nil
}

func runBridgeCommand(client bridge.Client, dbPath string, command bridge.Command) error {
	return runBridgeCommandWithExecute(client.Execute, dbPath, command)
}

func runBridgeCommandWithExecute(execute bridgeExecuteFunc, dbPath string, command bridge.Command) error {
	resp, err := execute(
		context.Background(),
		bridge.Request{
			DatabasePath: dbPath,
			Command:      command,
		},
	)
	if err != nil {
		return err
	}
	if err := bridge.ErrorFromResponse(resp); err != nil {
		return err
	}
	return nil
}

func queryScalarInt(sqliteBin, dbPath, query string) (int, error) {
	cmd := exec.Command(sqliteBin, dbPath, query)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return 0, fmt.Errorf("%w: %s", err, output)
	}
	value := strings.TrimSpace(string(output))
	if value == "" {
		return 0, nil
	}
	n, err := strconv.Atoi(value)
	if err != nil {
		return 0, fmt.Errorf("parse scalar %q: %w", value, err)
	}
	return n, nil
}

func splitRecoveredStatements(sql string) []string {
	statements := make([]string, 0, 32)
	var current strings.Builder
	var quote rune
	lineStart := true
	statementStarted := false

	flush := func() {
		statement := strings.TrimSpace(current.String())
		if statement != "" {
			statements = append(statements, statement)
		}
		current.Reset()
		statementStarted = false
	}

	for i := 0; i < len(sql); i++ {
		if quote == 0 && lineStart && !statementStarted {
			lineEnd := strings.IndexByte(sql[i:], '\n')
			if lineEnd < 0 {
				lineEnd = len(sql) - i
			}
			line := sql[i : i+lineEnd]
			if strings.HasPrefix(strings.ToLower(strings.TrimSpace(line)), "sql error:") {
				i += lineEnd
				lineStart = true
				continue
			}
		}

		ch := rune(sql[i])
		current.WriteByte(sql[i])
		if quote == 0 && !unicode.IsSpace(ch) {
			statementStarted = true
		}

		switch quote {
		case '\'':
			if ch == '\'' {
				if i+1 < len(sql) && sql[i+1] == '\'' {
					current.WriteByte(sql[i+1])
					i++
				} else {
					quote = 0
				}
			}
			continue
		case '"':
			if ch == '"' {
				if i+1 < len(sql) && sql[i+1] == '"' {
					current.WriteByte(sql[i+1])
					i++
				} else {
					quote = 0
				}
			}
			continue
		case '`':
			if ch == '`' {
				quote = 0
			}
			continue
		case '[':
			if ch == ']' {
				quote = 0
			}
			continue
		}

		switch ch {
		case '\'', '"', '`', '[':
			quote = ch
		case ';':
			flush()
		}
		lineStart = ch == '\n'
	}

	flush()
	return statements
}

func shouldSkipRecoveredStatement(statement string) bool {
	trimmed := strings.TrimSpace(statement)
	if trimmed == "" {
		return true
	}

	lower := strings.ToLower(trimmed)
	if strings.HasPrefix(lower, "sql error:") {
		return true
	}
	if strings.HasPrefix(lower, "pragma writable_schema") {
		return true
	}

	objectName, kind := recoveredStatementObject(trimmed)
	switch kind {
	case "sqlite_schema_insert":
		return true
	case "object":
		return isRecoveredProjectionObject(objectName) || isRecoveredSchemaCatalog(objectName)
	default:
		return false
	}
}

func recoveredStatementObject(statement string) (string, string) {
	trimmed := strings.TrimSpace(statement)
	lower := strings.ToLower(trimmed)

	switch {
	case strings.HasPrefix(lower, "insert into sqlite_schema"),
		strings.HasPrefix(lower, "insert into sqlite_master"),
		strings.HasPrefix(lower, "insert into \"sqlite_schema\""),
		strings.HasPrefix(lower, "insert into \"sqlite_master\""),
		strings.HasPrefix(lower, "insert into 'sqlite_schema'"),
		strings.HasPrefix(lower, "insert into 'sqlite_master'"):
		return "sqlite_schema", "sqlite_schema_insert"
	case strings.HasPrefix(lower, "create virtual table"):
		return extractObjectName(trimmed, "create virtual table"), "object"
	case strings.HasPrefix(lower, "create table"):
		return extractObjectName(trimmed, "create table"), "object"
	case strings.HasPrefix(lower, "insert "):
		if objectName := extractInsertObjectName(trimmed); objectName != "" {
			if objectName == "sqlite_schema" || objectName == "sqlite_master" {
				return "sqlite_schema", "sqlite_schema_insert"
			}
			return objectName, "object"
		}
		return "", ""
	case strings.HasPrefix(lower, "delete from"):
		return extractObjectName(trimmed, "delete from"), "object"
	case strings.HasPrefix(lower, "drop table"):
		return extractObjectName(trimmed, "drop table"), "object"
	default:
		return "", ""
	}
}

func extractObjectName(statement, prefix string) string {
	rest := strings.TrimSpace(statement[len(prefix):])
	lowerRest := strings.ToLower(rest)
	if strings.HasPrefix(lowerRest, "if not exists") {
		rest = strings.TrimSpace(rest[len("if not exists"):])
	}
	if rest == "" {
		return ""
	}

	identifier, _ := readRecoveredIdentifier(rest)
	parts := strings.Split(identifier, ".")
	return strings.ToLower(strings.TrimSpace(parts[len(parts)-1]))
}

func extractInsertObjectName(statement string) string {
	rest := strings.TrimSpace(statement[len("insert"):])
	lowerRest := strings.ToLower(rest)
	if strings.HasPrefix(lowerRest, "or ") {
		rest = strings.TrimSpace(rest[len("or "):])
		lowerRest = strings.ToLower(rest)
		if idx := strings.IndexFunc(lowerRest, unicode.IsSpace); idx >= 0 {
			rest = strings.TrimSpace(rest[idx:])
			lowerRest = strings.ToLower(rest)
		} else {
			return ""
		}
	}
	if !strings.HasPrefix(lowerRest, "into") {
		return ""
	}
	return extractObjectName("into "+strings.TrimSpace(rest[len("into"):]), "into")
}

func readRecoveredIdentifier(input string) (string, int) {
	if input == "" {
		return "", 0
	}

	switch input[0] {
	case '"', '\'', '`':
		quote := input[0]
		var ident strings.Builder
		for i := 1; i < len(input); i++ {
			if input[i] == quote {
				if i+1 < len(input) && input[i+1] == quote {
					ident.WriteByte(quote)
					i++
					continue
				}
				return ident.String(), i + 1
			}
			ident.WriteByte(input[i])
		}
	case '[':
		end := strings.IndexByte(input, ']')
		if end >= 0 {
			return input[1:end], end + 1
		}
	default:
		var ident strings.Builder
		for i, r := range input {
			if unicode.IsSpace(r) || r == '(' || r == ';' {
				return ident.String(), i
			}
			ident.WriteRune(r)
		}
		return ident.String(), len(input)
	}

	return "", 0
}

func isRecoveredProjectionObject(name string) bool {
	lower := strings.ToLower(strings.TrimSpace(name))
	return lower == "fts_nodes" || strings.HasPrefix(lower, "fts_nodes_") || lower == "vec_nodes_active"
}

func isRecoveredSchemaCatalog(name string) bool {
	lower := strings.ToLower(strings.TrimSpace(name))
	return lower == "sqlite_master" || lower == "sqlite_schema"
}
