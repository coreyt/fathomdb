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
	var layer2 *sqlitecheck.Layer2Report
	if bridgePath != "" {
		l2, err := fetchLayer2(databasePath, bridgePath)
		if err != nil {
			return fmt.Errorf("layer2 bridge call failed: %w", err)
		}
		layer2 = &l2
	}

	report, err := sqlitecheck.Diagnose(databasePath, "", layer2)
	if err != nil {
		return err
	}
	jsonStr, err := sqlitecheck.FormatDiagnostic(report)
	if err != nil {
		return err
	}
	fmt.Fprintln(out, jsonStr)
	fmt.Fprintln(out, "check completed")
	return nil
}

// bridgeIntegrityReport mirrors the Rust IntegrityReport JSON shape.
type bridgeIntegrityReport struct {
	PhysicalOK                bool     `json:"physical_ok"`
	ForeignKeysOK             bool     `json:"foreign_keys_ok"`
	MissingFTSRows            int      `json:"missing_fts_rows"`
	DuplicateActiveLogicalIDs int      `json:"duplicate_active_logical_ids"`
	Warnings                  []string `json:"warnings"`
}

// bridgeSemanticReport mirrors the Rust SemanticReport JSON shape.
type bridgeSemanticReport struct {
	OrphanedChunks     int      `json:"orphaned_chunks"`
	NullSourceRefNodes int      `json:"null_source_ref_nodes"`
	BrokenStepFK       int      `json:"broken_step_fk"`
	BrokenActionFK     int      `json:"broken_action_fk"`
	Warnings           []string `json:"warnings"`
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

	layer2 := sqlitecheck.Layer2Report{
		Available:                 true,
		PhysicalOK:                ir.PhysicalOK,
		ForeignKeysOK:             ir.ForeignKeysOK,
		MissingFTSRows:            ir.MissingFTSRows,
		DuplicateActiveLogicalIDs: ir.DuplicateActiveLogicalIDs,
		BrokenStepFK:              sr.BrokenStepFK,
		BrokenActionFK:            sr.BrokenActionFK,
		Findings:                  []sqlitecheck.Finding{},
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

	return layer2, nil
}
