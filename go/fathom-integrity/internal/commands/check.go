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
	_, err := bridge.RunWithFeedback(
		context.Background(),
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
				return struct{}{}, err
			}
			jsonStr, err := sqlitecheck.FormatDiagnostic(report)
			if err != nil {
				return struct{}{}, err
			}
			fmt.Fprintln(out, jsonStr)
			fmt.Fprintln(out, "check completed")
			return struct{}{}, nil
		},
	)
	return err
}

// bridgeIntegrityReport mirrors the Rust IntegrityReport JSON shape.
type bridgeIntegrityReport struct {
	PhysicalOK                      bool     `json:"physical_ok"`
	ForeignKeysOK                   bool     `json:"foreign_keys_ok"`
	MissingFTSRows                  int      `json:"missing_fts_rows"`
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
		return sqlitecheck.Layer2Report{}, err
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
		return sqlitecheck.Layer2Report{}, err
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

	return layer2
}
