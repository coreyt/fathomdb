package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/sqlitecheck"
)

// RunCheck runs a layered diagnostic on databasePath.  If bridgePath is
// non-empty the admin bridge is called to populate Layer 2 engine invariants.
func RunCheck(databasePath, bridgePath string, out io.Writer) error {
	return RunCheckWithFeedback(databasePath, bridgePath, out, nil, bridge.FeedbackConfig{})
}

// RunCheckWithFeedback is like RunCheck but emits lifecycle feedback events via the observer.
func RunCheckWithFeedback(
	databasePath, bridgePath string,
	out io.Writer,
	observer bridge.Observer,
	config bridge.FeedbackConfig,
) error {
	resolved := config.WithDefaults()
	ctx, cancel := context.WithTimeout(context.Background(), resolved.Timeout)
	defer cancel()

	_, err := bridge.RunWithFeedback(
		ctx,
		"go",
		"check",
		map[string]string{"database_path": databasePath},
		observer,
		config,
		func(context.Context) (struct{}, error) {
			var layer2 *sqlitecheck.Layer2Report
			if bridgePath != "" {
				l2, err := fetchLayer2(databasePath, bridgePath)
				if err != nil {
					return struct{}{}, fmt.Errorf("layer2 bridge call failed: %w", err)
				}
				layer2 = &l2
			}

			report, err := sqlitecheck.Diagnose(databasePath, "", layer2)
			if err != nil {
				return struct{}{}, fmt.Errorf("diagnose database: %w", err)
			}
			jsonStr, err := sqlitecheck.FormatDiagnostic(report)
			if err != nil {
				return struct{}{}, fmt.Errorf("format diagnostic report: %w", err)
			}
			fmt.Fprintln(out, jsonStr)
			fmt.Fprintln(out, "check completed")
			return struct{}{}, nil
		},
	)
	if err != nil {
		return fmt.Errorf("run check with feedback: %w", err)
	}
	return nil
}

// bridgeIntegrityReport mirrors the Rust IntegrityReport JSON shape.
type bridgeIntegrityReport struct {
	PhysicalOK                      bool     `json:"physical_ok"`
	ForeignKeysOK                   bool     `json:"foreign_keys_ok"`
	MissingFTSRows                  int      `json:"missing_fts_rows"`
	MissingPropertyFTSRows          int      `json:"missing_property_fts_rows"`
	DuplicateActiveLogicalIDs       int      `json:"duplicate_active_logical_ids"`
	OperationalMissingCollections   int      `json:"operational_missing_collections"`
	OperationalMissingLastMutations int      `json:"operational_missing_last_mutations"`
	Warnings                        []string `json:"warnings"`
}

// bridgeSemanticReport mirrors the Rust SemanticReport JSON shape.
type bridgeSemanticReport struct {
	OrphanedChunks                 int      `json:"orphaned_chunks"`
	NullSourceRefNodes             int      `json:"null_source_ref_nodes"`
	BrokenStepFK                   int      `json:"broken_step_fk"`
	BrokenActionFK                 int      `json:"broken_action_fk"`
	StaleFtsRows                   int      `json:"stale_fts_rows"`
	FtsRowsForSupersededNodes      int      `json:"fts_rows_for_superseded_nodes"`
	StalePropertyFtsRows           int      `json:"stale_property_fts_rows"`
	OrphanedPropertyFtsRows        int      `json:"orphaned_property_fts_rows"`
	MismatchedKindPropertyFtsRows  int      `json:"mismatched_kind_property_fts_rows"`
	DuplicatePropertyFtsRows       int      `json:"duplicate_property_fts_rows"`
	DriftedPropertyFtsRows         int      `json:"drifted_property_fts_rows"`
	DanglingEdges                  int      `json:"dangling_edges"`
	OrphanedSupersessionChains     int      `json:"orphaned_supersession_chains"`
	StaleVecRows                   int      `json:"stale_vec_rows"`
	VecRowsForSupersededNodes      int      `json:"vec_rows_for_superseded_nodes"`
	MissingOperationalCurrentRows  int      `json:"missing_operational_current_rows"`
	StaleOperationalCurrentRows    int      `json:"stale_operational_current_rows"`
	DisabledCollectionMutations    int      `json:"disabled_collection_mutations"`
	OrphanedLastAccessMetadataRows int      `json:"orphaned_last_access_metadata_rows"`
	Warnings                       []string `json:"warnings"`
}

func fetchLayer2(dbPath, bridgePath string) (sqlitecheck.Layer2Report, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	client := bridge.Client{BinaryPath: bridgePath}

	// --- check_integrity ---
	iresp, err := client.Execute(ctx, bridge.Request{
		DatabasePath: dbPath,
		Command:      bridge.CommandCheckIntegrity,
	})
	if err != nil {
		return sqlitecheck.Layer2Report{}, fmt.Errorf("execute check_integrity: %w", err)
	}
	if err := bridge.ErrorFromResponse(iresp); err != nil {
		return sqlitecheck.Layer2Report{}, fmt.Errorf("bridge check_integrity: %w", err)
	}
	var ir bridgeIntegrityReport
	if err := json.Unmarshal(iresp.Payload, &ir); err != nil {
		return sqlitecheck.Layer2Report{}, fmt.Errorf("decode integrity report: %w", err)
	}

	// --- check_semantics ---
	sresp, err := client.Execute(ctx, bridge.Request{
		DatabasePath: dbPath,
		Command:      bridge.CommandCheckSemantics,
	})
	if err != nil {
		return sqlitecheck.Layer2Report{}, fmt.Errorf("execute check_semantics: %w", err)
	}
	if err := bridge.ErrorFromResponse(sresp); err != nil {
		return sqlitecheck.Layer2Report{}, fmt.Errorf("bridge check_semantics: %w", err)
	}
	var sr bridgeSemanticReport
	if err := json.Unmarshal(sresp.Payload, &sr); err != nil {
		return sqlitecheck.Layer2Report{}, fmt.Errorf("decode semantic report: %w", err)
	}

	return buildLayer2Report(ir, sr), nil
}

func buildLayer2Report(
	ir bridgeIntegrityReport,
	sr bridgeSemanticReport,
) sqlitecheck.Layer2Report {
	layer2 := sqlitecheck.Layer2Report{
		Available:                      true,
		PhysicalOK:                     ir.PhysicalOK,
		ForeignKeysOK:                  ir.ForeignKeysOK,
		MissingFTSRows:                 ir.MissingFTSRows,
		MissingPropertyFTSRows:         ir.MissingPropertyFTSRows,
		DuplicateActiveLogicalIDs:      ir.DuplicateActiveLogicalIDs,
		BrokenStepFK:                   sr.BrokenStepFK,
		BrokenActionFK:                 sr.BrokenActionFK,
		OrphanedLastAccessMetadataRows: sr.OrphanedLastAccessMetadataRows,
		Findings:                       []sqlitecheck.Finding{},
	}

	if !ir.PhysicalOK {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error", Message: "engine: physical integrity check failed",
		})
	}
	if !ir.ForeignKeysOK {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error", Message: "engine: foreign key violations detected",
		})
	}
	if ir.DuplicateActiveLogicalIDs > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error",
			Message: fmt.Sprintf("%d logical_id(s) with multiple active rows", ir.DuplicateActiveLogicalIDs),
		})
	}
	if ir.MissingFTSRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d missing FTS projection(s) detected by engine", ir.MissingFTSRows),
		})
	}
	if ir.MissingPropertyFTSRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d missing property FTS projection(s) detected by engine", ir.MissingPropertyFTSRows),
		})
	}
	if ir.OperationalMissingCollections > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error",
			Message: fmt.Sprintf("%d operational row(s) reference missing collections", ir.OperationalMissingCollections),
		})
	}
	if ir.OperationalMissingLastMutations > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error",
			Message: fmt.Sprintf("%d operational_current row(s) reference missing last mutations", ir.OperationalMissingLastMutations),
		})
	}
	if sr.BrokenStepFK > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error",
			Message: fmt.Sprintf("%d step(s) with broken run_id FK", sr.BrokenStepFK),
		})
	}
	if sr.BrokenActionFK > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error",
			Message: fmt.Sprintf("%d action(s) with broken step_id FK", sr.BrokenActionFK),
		})
	}
	if sr.MissingOperationalCurrentRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d latest-state key(s) missing operational_current rows", sr.MissingOperationalCurrentRows),
		})
	}
	if sr.StaleOperationalCurrentRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d stale operational_current row(s)", sr.StaleOperationalCurrentRows),
		})
	}
	if sr.DisabledCollectionMutations > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d operational mutation(s) occurred after collection disable", sr.DisabledCollectionMutations),
		})
	}
	if sr.OrphanedLastAccessMetadataRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d orphaned last_access metadata row(s)", sr.OrphanedLastAccessMetadataRows),
		})
	}
	if sr.StaleFtsRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d stale FTS row(s)", sr.StaleFtsRows),
		})
	}
	if sr.FtsRowsForSupersededNodes > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d FTS row(s) for superseded nodes", sr.FtsRowsForSupersededNodes),
		})
	}
	if sr.DanglingEdges > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "error",
			Message: fmt.Sprintf("%d dangling edge(s)", sr.DanglingEdges),
		})
	}
	if sr.OrphanedSupersessionChains > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d orphaned supersession chain(s)", sr.OrphanedSupersessionChains),
		})
	}
	if sr.StaleVecRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d stale vec row(s)", sr.StaleVecRows),
		})
	}
	if sr.VecRowsForSupersededNodes > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d vec row(s) for superseded nodes", sr.VecRowsForSupersededNodes),
		})
	}
	if sr.StalePropertyFtsRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d stale property FTS row(s) for superseded/missing nodes", sr.StalePropertyFtsRows),
		})
	}
	if sr.OrphanedPropertyFtsRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d orphaned property FTS row(s) for unregistered kinds", sr.OrphanedPropertyFtsRows),
		})
	}
	if sr.MismatchedKindPropertyFtsRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d property FTS row(s) with kind mismatch against active node", sr.MismatchedKindPropertyFtsRows),
		})
	}
	if sr.DuplicatePropertyFtsRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d active logical ID(s) with duplicate property FTS rows", sr.DuplicatePropertyFtsRows),
		})
	}
	if sr.DriftedPropertyFtsRows > 0 {
		layer2.Findings = append(layer2.Findings, sqlitecheck.Finding{
			Layer: 2, Severity: "warning",
			Message: fmt.Sprintf("%d property FTS row(s) with stale text_content", sr.DriftedPropertyFtsRows),
		})
	}

	return layer2
}
