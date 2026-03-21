package commands

import (
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
)

func RunCheck(databasePath string, out io.Writer) error {
	report, err := sqlitecheck.Run(databasePath)
	if err != nil {
		return err
	}
	_, err = fmt.Fprintln(out, sqlitecheck.Format(report))
	return err
}
