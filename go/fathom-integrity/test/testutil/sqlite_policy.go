package testutil

import (
	"bufio"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

type SQLitePolicy struct {
	MinimumSupportedVersion string
	RepoDevVersion          string
	RepoLocalBinaryRelPath  string
}

func RepoRoot() string {
	_, filename, _, ok := runtime.Caller(0)
	if !ok {
		return "."
	}

	return filepath.Clean(filepath.Join(filepath.Dir(filename), "..", "..", "..", ".."))
}

func LoadSQLitePolicy() (SQLitePolicy, error) {
	repoRoot := RepoRoot()
	policyPath := filepath.Join(repoRoot, "tooling", "sqlite.env")
	file, err := os.Open(policyPath)
	if err != nil {
		return SQLitePolicy{}, err
	}
	defer file.Close()

	var policy SQLitePolicy
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		key, value, ok := strings.Cut(line, "=")
		if !ok {
			return SQLitePolicy{}, os.ErrInvalid
		}

		switch strings.TrimSpace(key) {
		case "SQLITE_MIN_VERSION":
			policy.MinimumSupportedVersion = strings.TrimSpace(value)
		case "SQLITE_VERSION":
			policy.RepoDevVersion = strings.TrimSpace(value)
		default:
			return SQLitePolicy{}, os.ErrInvalid
		}
	}
	if err := scanner.Err(); err != nil {
		return SQLitePolicy{}, err
	}
	if policy.MinimumSupportedVersion == "" || policy.RepoDevVersion == "" {
		return SQLitePolicy{}, os.ErrInvalid
	}

	policy.RepoLocalBinaryRelPath = filepath.Join(
		".local",
		"sqlite-"+policy.RepoDevVersion,
		"bin",
		"sqlite3",
	)
	return policy, nil
}

func SQLiteBinary() string {
	repoRoot := RepoRoot()
	policy, err := LoadSQLitePolicy()
	if err != nil {
		return "sqlite3"
	}

	projectSQLite, err := filepath.Abs(filepath.Join(repoRoot, policy.RepoLocalBinaryRelPath))
	if err == nil {
		if _, statErr := os.Stat(projectSQLite); statErr == nil {
			return projectSQLite
		}
	}

	return "sqlite3"
}
