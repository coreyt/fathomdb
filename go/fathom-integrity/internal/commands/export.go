package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

// RunExport exports a fathomdb database to destinationPath using the Rust admin
// bridge-backed SQLite backup path. When forceCheckpoint is true, the bridge
// requests a full WAL checkpoint before exporting; when false, export still
// produces a correct snapshot without requiring readers to clear first.
// bridgePath must point to the fathomdb-admin-bridge binary; if empty, the
// function returns an error immediately rather than falling back to a naive copy.
func RunExport(databasePath, destinationPath, bridgePath string, forceCheckpoint bool, out io.Writer) error {
	return RunExportWithFeedback(
		databasePath,
		destinationPath,
		bridgePath,
		forceCheckpoint,
		out,
		nil,
		bridge.FeedbackConfig{},
	)
}

func RunExportWithFeedback(
	databasePath, destinationPath, bridgePath string,
	forceCheckpoint bool,
	out io.Writer,
	observer bridge.Observer,
	config bridge.FeedbackConfig,
) error {
	_, err := bridge.RunWithFeedback(
		context.Background(),
		"go",
		"export",
		map[string]string{
			"database_path":    databasePath,
			"destination_path": destinationPath,
		},
		observer,
		config,
		func(ctx context.Context) (struct{}, error) {
			if bridgePath == "" {
				return struct{}{}, fmt.Errorf(
					"safe export requires the admin bridge binary (--bridge); " +
						"there is no supported fallback copy path")
			}

			// Security fix M-1: Use restrictive permissions for the destination directory
			// so other users on a shared system cannot read exported database files.
			if err := os.MkdirAll(filepath.Dir(destinationPath), 0o700); err != nil {
				return struct{}{}, fmt.Errorf("creating destination directory: %w", err)
			}

			client := bridge.Client{BinaryPath: bridgePath}
			resp, err := client.SafeExport(ctx, databasePath, destinationPath, forceCheckpoint)
			if err != nil {
				return struct{}{}, fmt.Errorf("safe_export bridge call failed: %w", err)
			}
			if err := bridge.ErrorFromResponse(resp); err != nil {
				return struct{}{}, fmt.Errorf("safe_export failed: %w", err)
			}

			var manifest bridge.ExportManifest
			if err := json.Unmarshal(resp.Payload, &manifest); err != nil {
				return struct{}{}, fmt.Errorf("parsing export manifest: %w", err)
			}

			exportedAt := time.Unix(manifest.ExportedAt, 0).UTC().Format(time.RFC3339)
			fmt.Fprintf(out, "exported  %s\n", destinationPath)
			fmt.Fprintf(out, "manifest  %s.export-manifest.json\n", destinationPath)
			fmt.Fprintf(out, "sha256    %s\n", manifest.SHA256)
			fmt.Fprintf(out, "pages     %d\n", manifest.PageCount)
			fmt.Fprintf(out, "schema    v%d\n", manifest.SchemaVersion)
			fmt.Fprintf(out, "at        %s\n", exportedAt)
			return struct{}{}, nil
		},
	)
	return err
}
