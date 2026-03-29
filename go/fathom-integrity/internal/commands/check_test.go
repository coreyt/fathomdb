package commands

import (
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
	"github.com/stretchr/testify/require"
)

func TestBuildLayer2ReportIncludesOrphanedLastAccessMetadataFinding(t *testing.T) {
	report := buildLayer2Report(
		bridgeIntegrityReport{
			PhysicalOK:    true,
			ForeignKeysOK: true,
		},
		bridgeSemanticReport{
			OrphanedLastAccessMetadataRows: 2,
		},
	)

	require.Equal(t, 2, report.OrphanedLastAccessMetadataRows)
	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer:    2,
		Severity: "warning",
		Message:  "2 orphaned last_access metadata row(s)",
	})
}
