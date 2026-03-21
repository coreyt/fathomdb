package sqlitecheck

import (
	"fmt"
	"io"
	"os"
)

const sqliteHeader = "SQLite format 3\x00"

type Report struct {
	Path        string
	HeaderValid bool
	SizeBytes   int64
	Warnings    []string
}

func Run(path string) (Report, error) {
	file, err := os.Open(path)
	if err != nil {
		return Report{}, err
	}
	defer file.Close()

	header := make([]byte, len(sqliteHeader))
	if _, err := io.ReadFull(file, header); err != nil {
		return Report{}, err
	}

	info, err := file.Stat()
	if err != nil {
		return Report{}, err
	}

	report := Report{
		Path:        path,
		HeaderValid: string(header) == sqliteHeader,
		SizeBytes:   info.Size(),
	}
	if !report.HeaderValid {
		report.Warnings = append(report.Warnings, "file header does not match SQLite format 3")
	}
	if report.SizeBytes < 100 {
		report.Warnings = append(report.Warnings, "database file is unusually small")
	}
	return report, nil
}

func Format(report Report) string {
	return fmt.Sprintf(
		"path=%s header_valid=%t size_bytes=%d warnings=%d",
		report.Path,
		report.HeaderValid,
		report.SizeBytes,
		len(report.Warnings),
	)
}
