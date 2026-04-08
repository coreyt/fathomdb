package commands

import (
	"encoding/json"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
	"github.com/stretchr/testify/require"
)

func TestSemanticReportUnmarshalAllFields(t *testing.T) {
	payload := `{
		"orphaned_chunks": 1,
		"null_source_ref_nodes": 2,
		"broken_step_fk": 3,
		"broken_action_fk": 4,
		"stale_fts_rows": 5,
		"fts_rows_for_superseded_nodes": 6,
		"dangling_edges": 7,
		"orphaned_supersession_chains": 8,
		"stale_vec_rows": 9,
		"vec_rows_for_superseded_nodes": 10,
		"missing_operational_current_rows": 11,
		"stale_operational_current_rows": 12,
		"disabled_collection_mutations": 13,
		"orphaned_last_access_metadata_rows": 14,
		"warnings": ["w1"]
	}`

	var sr bridgeSemanticReport
	require.NoError(t, json.Unmarshal([]byte(payload), &sr))

	require.Equal(t, 1, sr.OrphanedChunks)
	require.Equal(t, 2, sr.NullSourceRefNodes)
	require.Equal(t, 3, sr.BrokenStepFK)
	require.Equal(t, 4, sr.BrokenActionFK)
	require.Equal(t, 5, sr.StaleFtsRows)
	require.Equal(t, 6, sr.FtsRowsForSupersededNodes)
	require.Equal(t, 7, sr.DanglingEdges)
	require.Equal(t, 8, sr.OrphanedSupersessionChains)
	require.Equal(t, 9, sr.StaleVecRows)
	require.Equal(t, 10, sr.VecRowsForSupersededNodes)
	require.Equal(t, 11, sr.MissingOperationalCurrentRows)
	require.Equal(t, 12, sr.StaleOperationalCurrentRows)
	require.Equal(t, 13, sr.DisabledCollectionMutations)
	require.Equal(t, 14, sr.OrphanedLastAccessMetadataRows)
	require.Equal(t, []string{"w1"}, sr.Warnings)
}

func TestBuildLayer2ReportNewSemanticFindings(t *testing.T) {
	report := buildLayer2Report(
		bridgeIntegrityReport{
			PhysicalOK:    true,
			ForeignKeysOK: true,
		},
		bridgeSemanticReport{
			StaleFtsRows:               3,
			FtsRowsForSupersededNodes:  2,
			DanglingEdges:              1,
			OrphanedSupersessionChains: 4,
			StaleVecRows:               5,
			VecRowsForSupersededNodes:  6,
		},
	)

	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer: 2, Severity: "warning", Message: "3 stale FTS row(s)",
	})
	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer: 2, Severity: "warning", Message: "2 FTS row(s) for superseded nodes",
	})
	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer: 2, Severity: "error", Message: "1 dangling edge(s)",
	})
	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer: 2, Severity: "warning", Message: "4 orphaned supersession chain(s)",
	})
	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer: 2, Severity: "warning", Message: "5 stale vec row(s)",
	})
	require.Contains(t, report.Findings, sqlitecheck.Finding{
		Layer: 2, Severity: "warning", Message: "6 vec row(s) for superseded nodes",
	})
}

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
