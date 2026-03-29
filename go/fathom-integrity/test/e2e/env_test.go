package e2e

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/stretchr/testify/require"
)

var (
	sharedToolchainEnvOnce sync.Once
	sharedToolchainEnvRoot string
	sharedToolchainEnvErr  error
)

func commandEnv(t *testing.T, extra ...string) []string {
	t.Helper()

	root := sharedToolchainEnv(t)
	env := append([]string{}, os.Environ()...)
	env = setEnv(env, "CARGO_HOME", filepath.Join(root, "cargo-home"))
	env = setEnv(env, "CARGO_TARGET_DIR", filepath.Join(root, "cargo-target"))
	env = setEnv(env, "GOCACHE", filepath.Join(root, "go-build"))

	for _, kv := range extra {
		parts := strings.SplitN(kv, "=", 2)
		require.Lenf(t, parts, 2, "invalid env assignment %q", kv)
		env = setEnv(env, parts[0], parts[1])
	}

	return env
}

func sharedToolchainEnv(t *testing.T) string {
	t.Helper()

	sharedToolchainEnvOnce.Do(func() {
		root, err := os.MkdirTemp("", "fathomdb-e2e-toolchain-")
		if err != nil {
			sharedToolchainEnvErr = fmt.Errorf("create shared toolchain env: %w", err)
			return
		}

		if err := bootstrapCargoHome(root); err != nil {
			sharedToolchainEnvErr = err
			return
		}

		sharedToolchainEnvRoot = root
	})

	require.NoError(t, sharedToolchainEnvErr)
	return sharedToolchainEnvRoot
}

func bootstrapCargoHome(root string) error {
	cargoHome := filepath.Join(root, "cargo-home")
	if err := os.MkdirAll(filepath.Join(cargoHome, "registry", "src"), 0o755); err != nil {
		return fmt.Errorf("create cargo registry src: %w", err)
	}
	if err := os.MkdirAll(filepath.Join(root, "cargo-target"), 0o755); err != nil {
		return fmt.Errorf("create cargo target dir: %w", err)
	}
	if err := os.MkdirAll(filepath.Join(root, "go-build"), 0o755); err != nil {
		return fmt.Errorf("create go build cache: %w", err)
	}

	sourceHome, ok := ambientCargoHome()
	if !ok {
		return nil
	}

	if err := mirrorCargoSubdir(sourceHome, cargoHome, "registry", "cache"); err != nil {
		return err
	}
	if err := mirrorCargoSubdir(sourceHome, cargoHome, "registry", "index"); err != nil {
		return err
	}
	if err := mirrorCargoSubdir(sourceHome, cargoHome, "git"); err != nil {
		return err
	}

	return nil
}

func ambientCargoHome() (string, bool) {
	if cargoHome := os.Getenv("CARGO_HOME"); cargoHome != "" {
		if info, err := os.Stat(cargoHome); err == nil && info.IsDir() {
			return cargoHome, true
		}
	}

	homeDir, err := os.UserHomeDir()
	if err != nil {
		return "", false
	}
	cargoHome := filepath.Join(homeDir, ".cargo")
	if info, err := os.Stat(cargoHome); err == nil && info.IsDir() {
		return cargoHome, true
	}
	return "", false
}

func mirrorCargoSubdir(sourceHome, destHome string, parts ...string) error {
	sourcePath := filepath.Join(append([]string{sourceHome}, parts...)...)
	info, err := os.Stat(sourcePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("stat cargo cache path %s: %w", sourcePath, err)
	}
	if !info.IsDir() {
		return nil
	}

	destPath := filepath.Join(append([]string{destHome}, parts...)...)
	if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
		return fmt.Errorf("create cargo mirror parent %s: %w", destPath, err)
	}

	if _, err := os.Lstat(destPath); err == nil {
		return nil
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("stat cargo mirror destination %s: %w", destPath, err)
	}

	if err := os.Symlink(sourcePath, destPath); err != nil {
		return fmt.Errorf("symlink cargo cache %s -> %s: %w", destPath, sourcePath, err)
	}
	return nil
}

func setEnv(env []string, key, value string) []string {
	prefix := key + "="
	for i, item := range env {
		if strings.HasPrefix(item, prefix) {
			env[i] = prefix + value
			return env
		}
	}
	return append(env, prefix+value)
}

func TestCommandEnvDoesNotForceOfflineCargo(t *testing.T) {
	env := commandEnv(t)
	for _, item := range env {
		require.NotEqual(t, "CARGO_NET_OFFLINE=true", item)
	}
}
