package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"strings"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

const (
	RepairTargetAll             = "all"
	RepairTargetDuplicateActive = "duplicate-active"
	RepairTargetRuntimeFK       = "runtime-fk"
	RepairTargetOrphanedChunks  = "orphaned-chunks"
)

type RepairReport struct {
	DatabasePath string               `json:"database_path"`
	Target       string               `json:"target"`
	DryRun       bool                 `json:"dry_run"`
	Duplicate    DuplicateRepairStats `json:"duplicate_active"`
	RuntimeFK    RuntimeRepairStats   `json:"runtime_fk"`
	Orphaned     OrphanedRepairStats  `json:"orphaned_chunks"`
}

type DuplicateRepairStats struct {
	LogicalIDsRepaired int `json:"logical_ids_repaired"`
	RowsSuperseded     int `json:"rows_superseded"`
}

type RuntimeRepairStats struct {
	StepsDeleted   int `json:"steps_deleted"`
	ActionsDeleted int `json:"actions_deleted"`
}

type OrphanedRepairStats struct {
	ChunksDeleted int `json:"chunks_deleted"`
	FTSDeleted    int `json:"fts_deleted"`
	VecDeleted    int `json:"vec_deleted"`
}

func RunRepair(databasePath, bridgePath, sqliteBin, target string, dryRun bool, out io.Writer) error {
	return RunRepairWithFeedback(databasePath, bridgePath, sqliteBin, target, dryRun, out, nil, bridge.FeedbackConfig{})
}

func RunRepairWithFeedback(
	databasePath, bridgePath, sqliteBin, target string,
	dryRun bool,
	out io.Writer,
	observer bridge.Observer,
	config bridge.FeedbackConfig,
) error {
	if target == "" {
		target = RepairTargetAll
	}
	if err := validateRepairTarget(target); err != nil {
		return err
	}

	_, err := bridge.RunWithFeedback(
		context.Background(),
		"go",
		"repair",
		map[string]string{
			"database_path": databasePath,
			"target":        target,
		},
		observer,
		config,
		func(context.Context) (struct{}, error) {
			report, err := runRepair(databasePath, sqliteBin, target, dryRun)
			if err != nil {
				return struct{}{}, err
			}
			payload, err := json.Marshal(report)
			if err != nil {
				return struct{}{}, fmt.Errorf("marshal repair report: %w", err)
			}
			fmt.Fprintln(out, string(payload))
			fmt.Fprintln(out, "repair completed")
			return struct{}{}, nil
		},
	)
	return err
}

func runRepair(databasePath, sqliteBin, target string, dryRun bool) (RepairReport, error) {
	if sqliteBin == "" {
		bin, err := exec.LookPath("sqlite3")
		if err != nil {
			return RepairReport{}, fmt.Errorf("sqlite3 not found in PATH: %w", err)
		}
		sqliteBin = bin
	}

	report := RepairReport{
		DatabasePath: databasePath,
		Target:       target,
		DryRun:       dryRun,
	}

	runTarget := func(name string, fn func(string, bool) error) error {
		if target != RepairTargetAll && target != name {
			return nil
		}
		return fn(sqliteBin, dryRun)
	}

	if err := runTarget(RepairTargetDuplicateActive, func(sqliteBin string, dryRun bool) error {
		logicalIDs, err := runSQLiteCount(sqliteBin, databasePath, `
SELECT count(*) FROM (
    SELECT logical_id
    FROM nodes
    WHERE superseded_at IS NULL
    GROUP BY logical_id
    HAVING count(*) > 1
);`)
		if err != nil {
			return fmt.Errorf("count duplicate logical_ids: %w", err)
		}
		rowsSuperseded, err := runSQLiteCount(sqliteBin, databasePath, `
SELECT count(*) FROM nodes n
WHERE n.superseded_at IS NULL
  AND EXISTS (
      SELECT 1
      FROM nodes n2
      WHERE n2.logical_id = n.logical_id AND n2.superseded_at IS NULL
      GROUP BY n2.logical_id
      HAVING count(*) > 1
  )
  AND n.row_id != (
      SELECT n3.row_id
      FROM nodes n3
      WHERE n3.logical_id = n.logical_id AND n3.superseded_at IS NULL
      ORDER BY n3.created_at DESC, n3.row_id DESC
      LIMIT 1
  );`)
		if err != nil {
			return fmt.Errorf("count duplicate rows to supersede: %w", err)
		}
		report.Duplicate = DuplicateRepairStats{
			LogicalIDsRepaired: logicalIDs,
			RowsSuperseded:     rowsSuperseded,
		}
		if dryRun || rowsSuperseded == 0 {
			return nil
		}
		sql := `
BEGIN IMMEDIATE;
WITH winners AS (
    SELECT logical_id, row_id, created_at
    FROM (
        SELECT logical_id, row_id, created_at,
               row_number() OVER (
                   PARTITION BY logical_id
                   ORDER BY created_at DESC, row_id DESC
               ) AS rn
        FROM nodes
        WHERE superseded_at IS NULL
    )
    WHERE rn = 1
),
losers AS (
    SELECT n.row_id, n.logical_id
    FROM nodes n
    JOIN winners w ON w.logical_id = n.logical_id
    WHERE n.superseded_at IS NULL AND n.row_id != w.row_id
)
INSERT INTO provenance_events (id, event_type, subject, source_ref)
SELECT lower(hex(randomblob(16))), 'repair_duplicate_active_node', row_id, 'repair:duplicate-active'
FROM losers;
WITH winners AS (
    SELECT logical_id, row_id, created_at
    FROM (
        SELECT logical_id, row_id, created_at,
               row_number() OVER (
                   PARTITION BY logical_id
                   ORDER BY created_at DESC, row_id DESC
               ) AS rn
        FROM nodes
        WHERE superseded_at IS NULL
    )
    WHERE rn = 1
)
UPDATE nodes
SET superseded_at = (
    SELECT created_at FROM winners WHERE winners.logical_id = nodes.logical_id
)
WHERE superseded_at IS NULL
  AND row_id != (
      SELECT row_id FROM winners WHERE winners.logical_id = nodes.logical_id
  )
  AND EXISTS (
      SELECT 1 FROM winners WHERE winners.logical_id = nodes.logical_id
  );
DROP INDEX IF EXISTS idx_nodes_active_logical_id;
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_active_logical_id
    ON nodes(logical_id)
    WHERE superseded_at IS NULL;
COMMIT;`
		if _, err := runSQLiteExec(sqliteBin, databasePath, sql); err != nil {
			return fmt.Errorf("repair duplicate active logical_ids: %w", err)
		}
		return nil
	}); err != nil {
		return RepairReport{}, err
	}

	if err := runTarget(RepairTargetRuntimeFK, func(sqliteBin string, dryRun bool) error {
		actionsDeleted, err := runSQLiteCount(sqliteBin, databasePath, brokenRuntimeActionsCountSQL)
		if err != nil {
			return fmt.Errorf("count broken actions: %w", err)
		}
		stepsDeleted, err := runSQLiteCount(sqliteBin, databasePath, brokenRuntimeStepsCountSQL)
		if err != nil {
			return fmt.Errorf("count broken steps: %w", err)
		}
		report.RuntimeFK = RuntimeRepairStats{
			StepsDeleted:   stepsDeleted,
			ActionsDeleted: actionsDeleted,
		}
		if dryRun || (actionsDeleted == 0 && stepsDeleted == 0) {
			return nil
		}
		sql := `
BEGIN IMMEDIATE;
INSERT INTO provenance_events (id, event_type, subject, source_ref)
SELECT lower(hex(randomblob(16))), 'repair_delete_broken_action', a.id, 'repair:runtime-fk'
FROM actions a
WHERE NOT EXISTS (
        SELECT 1 FROM steps s WHERE s.id = a.step_id
    )
   OR EXISTS (
        SELECT 1
        FROM steps s
        WHERE s.id = a.step_id
          AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.run_id)
   );
DELETE FROM actions
WHERE NOT EXISTS (
        SELECT 1 FROM steps s WHERE s.id = actions.step_id
    )
   OR EXISTS (
        SELECT 1
        FROM steps s
        WHERE s.id = actions.step_id
          AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.run_id)
   );
INSERT INTO provenance_events (id, event_type, subject, source_ref)
SELECT lower(hex(randomblob(16))), 'repair_delete_broken_step', s.id, 'repair:runtime-fk'
FROM steps s
WHERE NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.run_id);
DELETE FROM steps
WHERE NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = steps.run_id);
COMMIT;`
		if _, err := runSQLiteExec(sqliteBin, databasePath, sql); err != nil {
			return fmt.Errorf("repair broken runtime FK chains: %w", err)
		}
		return nil
	}); err != nil {
		return RepairReport{}, err
	}

	if err := runTarget(RepairTargetOrphanedChunks, func(sqliteBin string, dryRun bool) error {
		chunksDeleted, err := runSQLiteCount(sqliteBin, databasePath, orphanedChunksCountSQL)
		if err != nil {
			return fmt.Errorf("count orphaned chunks: %w", err)
		}
		ftsDeleted, err := runSQLiteCount(sqliteBin, databasePath, orphanedFTSCountSQL)
		if err != nil {
			return fmt.Errorf("count orphaned fts rows: %w", err)
		}
		hasVecTable, err := runSQLiteCount(sqliteBin, databasePath, "SELECT count(*) FROM sqlite_schema WHERE name = 'vec_nodes_active';")
		if err != nil {
			return fmt.Errorf("check vec table presence: %w", err)
		}
		vecDeleted := 0
		if hasVecTable > 0 {
			vecDeleted, err = runSQLiteCount(sqliteBin, databasePath, orphanedVecCountSQL)
			if err != nil {
				return fmt.Errorf("count orphaned vec rows: %w", err)
			}
		}
		report.Orphaned = OrphanedRepairStats{
			ChunksDeleted: chunksDeleted,
			FTSDeleted:    ftsDeleted,
			VecDeleted:    vecDeleted,
		}
		if dryRun || chunksDeleted == 0 {
			return nil
		}
		var sql strings.Builder
		sql.WriteString(`
BEGIN IMMEDIATE;
INSERT INTO provenance_events (id, event_type, subject, source_ref)
SELECT lower(hex(randomblob(16))), 'repair_delete_orphaned_chunk', c.id, 'repair:orphaned-chunks'
FROM chunks c
WHERE NOT EXISTS (
    SELECT 1 FROM nodes n
    WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
);
`)
		if hasVecTable > 0 {
			sql.WriteString(`
DELETE FROM vec_nodes_active
WHERE chunk_id IN (
    SELECT c.id
    FROM chunks c
    WHERE NOT EXISTS (
        SELECT 1 FROM nodes n
        WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
    )
);
`)
		}
		sql.WriteString(`
DELETE FROM fts_nodes
WHERE chunk_id IN (
    SELECT c.id
    FROM chunks c
    WHERE NOT EXISTS (
        SELECT 1 FROM nodes n
        WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
    )
);
DELETE FROM chunks
WHERE NOT EXISTS (
    SELECT 1 FROM nodes n
    WHERE n.logical_id = chunks.node_logical_id AND n.superseded_at IS NULL
);
COMMIT;`)
		if _, err := runSQLiteExec(sqliteBin, databasePath, sql.String()); err != nil {
			return fmt.Errorf("repair orphaned chunks: %w", err)
		}
		return nil
	}); err != nil {
		return RepairReport{}, err
	}

	return report, nil
}

var (
	brokenRuntimeStepsCountSQL = `
SELECT count(*)
FROM steps s
WHERE NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.run_id);`

	brokenRuntimeActionsCountSQL = `
SELECT count(*)
FROM actions a
WHERE NOT EXISTS (
        SELECT 1 FROM steps s WHERE s.id = a.step_id
    )
   OR EXISTS (
        SELECT 1
        FROM steps s
        WHERE s.id = a.step_id
          AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.run_id)
   );`

	orphanedChunksCountSQL = `
SELECT count(*)
FROM chunks c
WHERE NOT EXISTS (
    SELECT 1 FROM nodes n
    WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
);`

	orphanedFTSCountSQL = `
SELECT count(*)
FROM fts_nodes f
WHERE f.chunk_id IN (
    SELECT c.id
    FROM chunks c
    WHERE NOT EXISTS (
        SELECT 1 FROM nodes n
        WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
    )
);`

	orphanedVecCountSQL = `
SELECT count(*)
FROM vec_nodes_active v
WHERE v.chunk_id IN (
    SELECT c.id
    FROM chunks c
    WHERE NOT EXISTS (
        SELECT 1 FROM nodes n
        WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
    )
);`
)

func validateRepairTarget(target string) error {
	switch target {
	case RepairTargetAll, RepairTargetDuplicateActive, RepairTargetRuntimeFK, RepairTargetOrphanedChunks:
		return nil
	default:
		return bridge.BridgeError{
			Code: bridge.ErrorBadRequest,
			Message: fmt.Sprintf(
				"invalid repair target %q (expected one of: %s, %s, %s, %s)",
				target,
				RepairTargetAll,
				RepairTargetDuplicateActive,
				RepairTargetRuntimeFK,
				RepairTargetOrphanedChunks,
			),
		}
	}
}

func runSQLiteExec(sqliteBin, dbPath, sql string) (string, error) {
	cmd := exec.Command(sqliteBin, dbPath, sql)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("%w: %s", err, strings.TrimSpace(string(output)))
	}
	return string(output), nil
}

func runSQLiteCount(sqliteBin, dbPath, query string) (int, error) {
	output, err := runSQLiteExec(sqliteBin, dbPath, query)
	if err != nil {
		return 0, err
	}
	value := strings.TrimSpace(output)
	if value == "" {
		return 0, nil
	}
	var count int
	if _, err := fmt.Sscanf(value, "%d", &count); err != nil {
		return 0, fmt.Errorf("parse sqlite count %q: %w", value, err)
	}
	return count, nil
}
