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
// bridge to ensure the WAL is fully checkpointed before the copy.
// bridgePath must point to the fathomdb-admin-bridge binary; if empty, the
// function returns an error immediately rather than falling back to a naive copy.
func RunExport(databasePath, destinationPath, bridgePath string, out io.Writer) error {
	if bridgePath == "" {
		return fmt.Errorf(
			"safe export requires the admin bridge binary (--bridge); " +
				"without it the WAL cannot be checkpointed and the copy may be incomplete")
	}

	// Security fix M-1: Use restrictive permissions for the destination directory
	// so other users on a shared system cannot read exported database files.
	if err := os.MkdirAll(filepath.Dir(destinationPath), 0o700); err != nil {
		return fmt.Errorf("creating destination directory: %w", err)
	}

	client := bridge.Client{BinaryPath: bridgePath}
	resp, err := client.SafeExport(context.Background(), databasePath, destinationPath)
	if err != nil {
		return fmt.Errorf("safe_export bridge call failed: %w", err)
	}
	if err := bridge.ErrorFromResponse(resp); err != nil {
		return fmt.Errorf("safe_export failed: %w", err)
	}

	var manifest bridge.ExportManifest
	if err := json.Unmarshal(resp.Payload, &manifest); err != nil {
		return fmt.Errorf("parsing export manifest: %w", err)
	}

	exportedAt := time.Unix(manifest.ExportedAt, 0).UTC().Format(time.RFC3339)
	fmt.Fprintf(out, "exported  %s\n", destinationPath)
	fmt.Fprintf(out, "manifest  %s.export-manifest.json\n", destinationPath)
	fmt.Fprintf(out, "sha256    %s\n", manifest.SHA256)
	fmt.Fprintf(out, "pages     %d\n", manifest.PageCount)
	fmt.Fprintf(out, "schema    v%d\n", manifest.SchemaVersion)
	fmt.Fprintf(out, "at        %s\n", exportedAt)
	return nil
}
