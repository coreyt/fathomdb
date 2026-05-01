package config

import (
	"os"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestLoadUsesDefaults(t *testing.T) {
	t.Setenv("FATHOM_DB_PATH", "")
	t.Setenv("FATHOM_ADMIN_BRIDGE", "")

	cfg := Load()

	require.Equal(t, "", cfg.DatabasePath)
	require.Equal(t, defaultBridgeBinary, cfg.BridgeBinary)
}

func TestLoadUsesEnvironmentOverrides(t *testing.T) {
	t.Setenv("FATHOM_DB_PATH", "/tmp/fathom.db")
	t.Setenv("FATHOM_ADMIN_BRIDGE", "/tmp/fathomdb-admin-bridge")

	cfg := Load()

	require.Equal(t, "/tmp/fathom.db", cfg.DatabasePath)
	require.Equal(t, "/tmp/fathomdb-admin-bridge", cfg.BridgeBinary)
}

func TestDefaultStringFallsBack(t *testing.T) {
	require.Equal(t, "fallback", defaultString("", "fallback"))
	require.Equal(t, "value", defaultString("value", "fallback"))
}

func TestEnvironmentDoesNotLeak(t *testing.T) {
	require.NoError(t, os.Unsetenv("FATHOM_DB_PATH"))
	require.NoError(t, os.Unsetenv("FATHOM_ADMIN_BRIDGE"))
}
