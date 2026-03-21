package testutil

import (
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestLoadSQLitePolicy(t *testing.T) {
	policy, err := LoadSQLitePolicy()

	require.NoError(t, err)
	require.Equal(t, "3.41.0", policy.MinimumSupportedVersion)
	require.Equal(t, "3.46.0", policy.RepoDevVersion)
	require.Contains(t, policy.RepoLocalBinaryRelPath, "sqlite-3.46.0/bin/sqlite3")
}

func TestSQLiteBinaryPrefersRepoLocalInstall(t *testing.T) {
	sqlitePath := SQLiteBinary()

	require.Contains(t, sqlitePath, filepath.Join(".local", "sqlite-3.46.0", "bin", "sqlite3"))
}
