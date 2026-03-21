package sqlitecheck

import (
	"os"
	"path/filepath"
	"testing"

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
