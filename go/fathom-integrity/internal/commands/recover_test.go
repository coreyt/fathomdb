package commands

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

func TestSanitizeRecoveredSQL_RemovesWritableSchemaAndSqliteMasterWrites(t *testing.T) {
	input := "" +
		"sql error: database is locked (5)\n" +
		"BEGIN;\n" +
		"PRAGMA writable_schema = on;\n" +
		"INSERT INTO sqlite_schema VALUES('table', 'fts_nodes', 'fts_nodes', 0, 'CREATE VIRTUAL TABLE fts_nodes USING fts5(\n" +
		"  chunk_id UNINDEXED,\n" +
		"  text_content\n" +
		")');\n" +
		"CREATE TABLE 'fts_nodes_data'(id INTEGER PRIMARY KEY, block BLOB);\n" +
		"CREATE TABLE vector_profiles (\n" +
		"  profile TEXT PRIMARY KEY,\n" +
		"  table_name TEXT NOT NULL\n" +
		");\n" +
		"CREATE TABLE x(y);\n" +
		"INSERT INTO x VALUES(1);\n" +
		"PRAGMA writable_schema = off;\n" +
		"COMMIT;\n"

	output := sanitizeRecoveredSQL(input)

	require.NotContains(t, output, "sql error:")
	require.NotContains(t, output, "writable_schema")
	require.NotContains(t, output, "sqlite_master")
	require.NotContains(t, output, "sqlite_schema")
	require.NotContains(t, output, "fts_nodes")
	require.Contains(t, output, "CREATE TABLE vector_profiles")
	require.Contains(t, output, "CREATE TABLE x(y);")
	require.Contains(t, output, "INSERT INTO x VALUES(1);")
	require.Contains(t, output, "BEGIN;")
	require.Contains(t, output, "COMMIT;")
}

func TestSanitizeRecoveredSQL_RemovesFtsNodePropertiesShadowTables(t *testing.T) {
	input := "" +
		"BEGIN;\n" +
		"CREATE TABLE 'fts_node_properties_data'(id INTEGER PRIMARY KEY, block BLOB);\n" +
		"CREATE TABLE 'fts_node_properties_idx'(segid, term, pgno, PRIMARY KEY(segid, term)) WITHOUT ROWID;\n" +
		"CREATE TABLE 'fts_node_properties_content'(id INTEGER PRIMARY KEY, c0, c1, c2);\n" +
		"CREATE TABLE 'fts_node_properties_docsize'(id INTEGER PRIMARY KEY, sz BLOB);\n" +
		"CREATE TABLE 'fts_node_properties_config'(k PRIMARY KEY, v) WITHOUT ROWID;\n" +
		"INSERT INTO fts_node_properties_content VALUES(1, 'goal-1', 'Goal', 'Ship v2');\n" +
		"CREATE TABLE x(y);\n" +
		"COMMIT;\n"

	output := sanitizeRecoveredSQL(input)

	require.NotContains(t, output, "fts_node_properties")
	require.Contains(t, output, "CREATE TABLE x(y);")
}

func TestSanitizeRecoveredSQL_PreservesVectorProfilesAndUserDataWithReservedWords(t *testing.T) {
	input := "" +
		"BEGIN;\n" +
		"CREATE TABLE vector_profiles (\n" +
		"  profile TEXT PRIMARY KEY,\n" +
		"  table_name TEXT NOT NULL,\n" +
		"  dimension INTEGER NOT NULL,\n" +
		"  enabled INTEGER NOT NULL\n" +
		");\n" +
		"INSERT INTO vector_profiles VALUES('default', 'vec_nodes_active', 4, 1);\n" +
		"INSERT INTO chunks VALUES('chunk-1', 'node-1', 'text mentions fts_nodes and sqlite_schema and vector_profiles', 100, NULL, NULL);\n" +
		"INSERT INTO nodes VALUES('row-1', 'node-1', 'Document', '{\"note\":\"vec_nodes_active appears in json\"}', 100, NULL, 'src-1');\n" +
		"COMMIT;\n"

	output := sanitizeRecoveredSQL(input)

	require.Contains(t, output, "CREATE TABLE vector_profiles")
	require.Contains(t, output, "INSERT INTO vector_profiles VALUES('default', 'vec_nodes_active', 4, 1);")
	require.Contains(t, output, "text mentions fts_nodes and sqlite_schema and vector_profiles")
	require.Contains(t, output, "{\"note\":\"vec_nodes_active appears in json\"}")
}

func TestSanitizeRecoveredSQL_PreservesMultilineTextContainingSqlErrorPrefix(t *testing.T) {
	input := "" +
		"BEGIN;\n" +
		"INSERT INTO chunks VALUES('chunk-1', 'node-1', 'line 1\nsql error: preserved text inside chunk\nline 3', 100, NULL, NULL);\n" +
		"COMMIT;\n"

	output := sanitizeRecoveredSQL(input)

	require.Contains(t, output, "sql error: preserved text inside chunk")
	require.Contains(t, output, "INSERT INTO chunks")
}

func TestRunBridgeCommandWithExecuteHasDeadline(t *testing.T) {
	var sawDeadline bool
	var deadlineDuration time.Duration
	err := runBridgeCommandWithExecute(
		func(ctx context.Context, request bridge.Request) (bridge.Response, error) {
			deadline, ok := ctx.Deadline()
			sawDeadline = ok
			if ok {
				deadlineDuration = time.Until(deadline)
			}
			return bridge.Response{
				ProtocolVersion: bridge.ProtocolVersion,
				OK:              true,
				Message:         "ok",
			}, nil
		},
		"/tmp/fathom.db",
		bridge.CommandRebuildMissing,
	)

	require.NoError(t, err)
	require.True(t, sawDeadline, "recovery bridge calls must have a deadline")
	require.Greater(t, deadlineDuration, 9*time.Minute, "recovery bridge timeout should be ~10 minutes")
	require.LessOrEqual(t, deadlineDuration, 10*time.Minute+time.Second, "recovery bridge timeout should be ~10 minutes")
}

func TestCountRecoveredRowsIncludesOperationalTables(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	sourcePath := seedRecoverSourceDB(t, sqliteBin)
	destPath := filepath.Join(t.TempDir(), "recovered.db")
	var out bytes.Buffer

	err := runRecover(sourcePath, destPath, "", sqliteBin, &out)

	require.NoError(t, err, out.String())
	report := decodeRecoverReport(t, out.String())
	require.Equal(t, 1, report.RowCounts.OperationalCollections)
	require.Equal(t, 1, report.RowCounts.OperationalMutations)
	require.Equal(t, 1, report.RowCounts.OperationalCurrent)
}

func TestRunRecover_DestinationExistsFailsWithoutOverwrite(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	sourcePath := seedRecoverSourceDB(t, sqliteBin)
	destPath := filepath.Join(t.TempDir(), "recovered.db")
	require.NoError(t, os.WriteFile(destPath, []byte("existing"), 0o600))
	var out bytes.Buffer

	err := runRecover(sourcePath, destPath, "", sqliteBin, &out)

	require.Error(t, err)
	require.Contains(t, err.Error(), "destination already exists")
	contents, readErr := os.ReadFile(destPath)
	require.NoError(t, readErr)
	require.Equal(t, "existing", string(contents))
}

func TestRunRecover_DestinationSymlinkFailsWithoutOverwrite(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("symlink creation may require privileges on Windows")
	}
	sqliteBin := testutil.SQLiteBinary()
	sourcePath := seedRecoverSourceDB(t, sqliteBin)
	dir := t.TempDir()
	destPath := filepath.Join(dir, "recovered.db")
	require.NoError(t, os.Symlink(filepath.Join(dir, "target.db"), destPath))
	var out bytes.Buffer

	err := runRecover(sourcePath, destPath, "", sqliteBin, &out)

	require.Error(t, err)
	require.Contains(t, err.Error(), "destination already exists")
}

func TestRunRecover_DestinationCreatedBeforePublishIsNotOverwritten(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	sourcePath := seedRecoverSourceDB(t, sqliteBin)
	destPath := filepath.Join(t.TempDir(), "recovered.db")
	oldHook := beforeRecoverPublish
	beforeRecoverPublish = func() error {
		return os.WriteFile(destPath, []byte("race"), 0o600)
	}
	t.Cleanup(func() {
		beforeRecoverPublish = oldHook
	})
	var out bytes.Buffer

	err := runRecover(sourcePath, destPath, "", sqliteBin, &out)

	require.Error(t, err)
	require.Contains(t, err.Error(), "publish recovered database")
	require.Contains(t, err.Error(), "destination already exists")
	contents, readErr := os.ReadFile(destPath)
	require.NoError(t, readErr)
	require.Equal(t, "race", string(contents))
}

func TestRunRecover_BestEffortWithoutBridgeIgnoresCountFailures(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	sourcePath := seedRecoverSourceDB(t, sqliteBin)
	destPath := filepath.Join(t.TempDir(), "recovered.db")
	failingSQLite := makeFailingSQLiteWrapper(t, sqliteBin)
	var out bytes.Buffer

	err := runRecover(sourcePath, destPath, "", failingSQLite, &out)

	require.NoError(t, err, out.String())
	report := decodeRecoverReport(t, out.String())
	require.Equal(t, 1, report.RowCounts.OperationalCollections)
	require.Equal(t, 1, report.RowCounts.OperationalMutations)
	require.Equal(t, 0, report.RowCounts.OperationalCurrent)
}

func TestRunRecover_BridgeBackedCountFailuresAreFatal(t *testing.T) {
	sqliteBin := testutil.SQLiteBinary()
	sourcePath := seedRecoverSourceDB(t, sqliteBin)
	destPath := filepath.Join(t.TempDir(), "recovered.db")
	failingSQLite := makeFailingSQLiteWrapper(t, sqliteBin)
	bridgePath := makeSuccessBridgeScript(t)
	var out bytes.Buffer

	err := runRecover(sourcePath, destPath, bridgePath, failingSQLite, &out)

	require.Error(t, err)
	require.Contains(t, err.Error(), "count recovered operational_current rows")
	_, statErr := os.Lstat(destPath)
	require.True(t, os.IsNotExist(statErr), "failed bridge-backed recovery must not publish dest")
}

func seedRecoverSourceDB(t *testing.T, sqliteBin string) string {
	t.Helper()

	dir := t.TempDir()
	dbPath := filepath.Join(dir, "source.db")
	sql := strings.Join([]string{
		"CREATE TABLE nodes (id TEXT);",
		"CREATE TABLE chunks (id TEXT);",
		"CREATE TABLE runs (id TEXT);",
		"CREATE TABLE steps (id TEXT);",
		"CREATE TABLE actions (id TEXT);",
		"CREATE TABLE vector_profiles (profile TEXT, enabled INTEGER);",
		"CREATE TABLE operational_collections (name TEXT);",
		"CREATE TABLE operational_mutations (id TEXT);",
		"CREATE TABLE operational_current (record_key TEXT);",
		"INSERT INTO nodes VALUES ('node-1');",
		"INSERT INTO chunks VALUES ('chunk-1');",
		"INSERT INTO runs VALUES ('run-1');",
		"INSERT INTO steps VALUES ('step-1');",
		"INSERT INTO actions VALUES ('action-1');",
		"INSERT INTO vector_profiles VALUES ('default', 0);",
		"INSERT INTO operational_collections VALUES ('audit_log');",
		"INSERT INTO operational_mutations VALUES ('mut-1');",
		"INSERT INTO operational_current VALUES ('entry-1');",
	}, " ")

	cmd := exec.Command(sqliteBin, dbPath, sql)
	out, err := cmd.CombinedOutput()
	require.NoError(t, err, string(out))
	return dbPath
}

// makeFailingSQLiteWrapper creates a script that fails when invoked with
// arguments containing "operational_current", otherwise delegates to the real
// sqlite3 binary.  On Unix it creates a bash script; on Windows a .bat file.
func makeFailingSQLiteWrapper(t *testing.T, realSQLite string) string {
	t.Helper()

	dir := t.TempDir()
	if runtime.GOOS == "windows" {
		path := filepath.Join(dir, "sqlite-wrapper.bat")
		// %* is all arguments; use findstr to check for operational_current.
		script := "@echo off\r\n" +
			"echo %* | findstr /C:\"operational_current\" >nul\r\n" +
			"if %errorlevel%==0 (\r\n" +
			"  echo forced count failure for operational_current 1>&2\r\n" +
			"  exit /b 1\r\n" +
			")\r\n" +
			"\"" + realSQLite + "\" %*\r\n"
		require.NoError(t, os.WriteFile(path, []byte(script), 0o755))
		return path
	}
	path := filepath.Join(dir, "sqlite-wrapper.sh")
	script := "#!/usr/bin/env bash\nset -euo pipefail\ncase \"$*\" in\n  *operational_current*)\n    echo \"forced count failure for operational_current\" >&2\n    exit 1\n    ;;\nesac\nexec " + shellQuote(realSQLite) + " \"$@\"\n"
	require.NoError(t, os.WriteFile(path, []byte(script), 0o755)) //nolint:gosec // G306: test executable in t.TempDir()
	return path
}

// makeSuccessBridgeScript creates a script that consumes stdin and emits a
// successful bridge response.  On Unix it creates a bash script; on Windows
// a .bat file.
func makeSuccessBridgeScript(t *testing.T) string {
	t.Helper()

	dir := t.TempDir()
	if runtime.GOOS == "windows" {
		path := filepath.Join(dir, "bridge.bat")
		script := "@echo off\r\n" +
			"powershell -NoProfile -Command \"[System.Console]::In.ReadToEnd() | Out-Null\"\r\n" +
			"echo {\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}\r\n"
		require.NoError(t, os.WriteFile(path, []byte(script), 0o755))
		return path
	}
	path := filepath.Join(dir, "bridge.sh")
	script := "#!/usr/bin/env bash\nset -euo pipefail\ncat >/dev/null\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	require.NoError(t, os.WriteFile(path, []byte(script), 0o755))
	return path
}

func decodeRecoverReport(t *testing.T, output string) RecoverReport {
	t.Helper()

	firstLine := strings.TrimSpace(output)
	if idx := strings.IndexByte(firstLine, '\n'); idx >= 0 {
		firstLine = firstLine[:idx]
	}
	var report RecoverReport
	require.NoError(t, json.Unmarshal([]byte(firstLine), &report), output)
	return report
}

func shellQuote(value string) string {
	return "'" + strings.ReplaceAll(value, "'", "'\"'\"'") + "'"
}
