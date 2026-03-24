package commands

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
)

func RunExport(databasePath, destinationPath string, out io.Writer) error {
	// Security fix M-1: Use restrictive permissions for directories and files
	// containing database exports. 0o700 prevents other users on a shared
	// system from reading exported data.
	if err := os.MkdirAll(filepath.Dir(destinationPath), 0o700); err != nil {
		return err
	}
	if _, err := copyFile(destinationPath, databasePath); err != nil {
		return err
	}
	_, err := fmt.Fprintf(out, "exported %s -> %s\n", databasePath, destinationPath)
	return err
}

func copyFile(destinationPath, sourcePath string) (int64, error) {
	source, err := os.Open(sourcePath)
	if err != nil {
		return 0, err
	}
	defer source.Close()

	// Security fix M-1: Create exported database files with 0o600 (owner-only)
	// instead of the default 0o666 to prevent exposure on shared systems.
	destination, err := os.OpenFile(destinationPath, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, 0o600)
	if err != nil {
		return 0, err
	}

	n, err := io.Copy(destination, source)
	if err != nil {
		destination.Close()
		return n, err
	}
	if err := destination.Close(); err != nil {
		return n, err
	}
	return n, nil
}
