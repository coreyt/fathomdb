package commands

import (
	"context"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
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

func TestRunBridgeCommandDoesNotInstallFixedDeadline(t *testing.T) {
	var sawDeadline bool
	err := runBridgeCommandWithExecute(
		func(ctx context.Context, request bridge.Request) (bridge.Response, error) {
			var ok bool
			_, ok = ctx.Deadline()
			sawDeadline = ok
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
	require.False(t, sawDeadline, "recovery bridge restore path must not impose a fixed deadline")
}
