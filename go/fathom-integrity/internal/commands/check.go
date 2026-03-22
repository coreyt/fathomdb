package commands

import (
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
)

func RunCheck(databasePath string, out io.Writer) error {
	report, err := sqlitecheck.Diagnose(databasePath, "")
	if err != nil {
		return err
	}
	json, err := sqlitecheck.FormatDiagnostic(report)
	if err != nil {
		return err
	}
	fmt.Fprintln(out, json)
	fmt.Fprintln(out, "check completed")
	return nil
}
