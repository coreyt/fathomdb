package commands

import "testing"

func FuzzSanitizeRecoveredSQL_Idempotent(f *testing.F) {
	f.Add("BEGIN;\nCREATE TABLE x(y);\nINSERT INTO x VALUES(1);\nCOMMIT;\n")
	f.Add("sql error: database is locked (5)\nINSERT INTO sqlite_schema VALUES('table', 'fts_nodes', 'fts_nodes', 0, 'CREATE VIRTUAL TABLE fts_nodes USING fts5(text_content)');\n")
	f.Add("INSERT INTO chunks VALUES('chunk-1', 'node-1', 'line 1\nsql error: preserved text\nline 3', 100, NULL, NULL);\n")

	f.Fuzz(func(t *testing.T, input string) {
		sanitized := sanitizeRecoveredSQL(input)
		_ = splitRecoveredStatements(sanitized)
		if sanitizedAgain := sanitizeRecoveredSQL(sanitized); sanitizedAgain != sanitized {
			t.Fatalf("sanitizeRecoveredSQL must be idempotent")
		}
	})
}
