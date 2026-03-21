package commands

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
)

func RunExport(databasePath, destinationPath string, out io.Writer) error {
	if err := os.MkdirAll(filepath.Dir(destinationPath), 0o755); err != nil {
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

	destination, err := os.Create(destinationPath)
	if err != nil {
		return 0, err
	}
	defer destination.Close()

	return io.Copy(destination, source)
}
