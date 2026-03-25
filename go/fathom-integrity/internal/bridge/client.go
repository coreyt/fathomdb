package bridge

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

const ProtocolVersion = 1

type Command string

const (
	CommandCheckIntegrity     Command = "check_integrity"
	CommandCheckSemantics     Command = "check_semantics"
	CommandRebuildProjections Command = "rebuild_projections"
	CommandRebuildMissing     Command = "rebuild_missing_projections"
	CommandTraceSource        Command = "trace_source"
	CommandExciseSource       Command = "excise_source"
	CommandSafeExport         Command = "safe_export"
)

type Request struct {
	ProtocolVersion int     `json:"protocol_version"`
	DatabasePath    string  `json:"database_path"`
	Command         Command `json:"command"`
	Target          string  `json:"target,omitempty"`
	SourceRef       string  `json:"source_ref,omitempty"`
	DestinationPath string  `json:"destination_path,omitempty"`
}

type Response struct {
	ProtocolVersion int             `json:"protocol_version"`
	OK              bool            `json:"ok"`
	Message         string          `json:"message"`
	Payload         json.RawMessage `json:"payload"`
}

func (r Request) MarshalJSON() ([]byte, error) {
	type alias Request
	payload := alias(r)
	if payload.ProtocolVersion == 0 {
		payload.ProtocolVersion = ProtocolVersion
	}
	return json.Marshal(payload)
}

// ExportManifest is the structured payload returned by the bridge safe_export
// command and written as <destination>.export-manifest.json by the Rust engine.
type ExportManifest struct {
	ExportedAt      int64  `json:"exported_at"`
	SHA256          string `json:"sha256"`
	SchemaVersion   uint32 `json:"schema_version"`
	ProtocolVersion uint32 `json:"protocol_version"`
	PageCount       uint64 `json:"page_count"`
}

type Client struct {
	BinaryPath string
}

// Security fix H-3: validateBinaryPath checks that the bridge binary exists,
// uses an absolute path, and is not world-writable. This prevents execution of
// arbitrary binaries via the FATHOM_ADMIN_BRIDGE env var or --bridge flag.
func validateBinaryPath(path string) error {
	if !filepath.IsAbs(path) {
		return fmt.Errorf("bridge binary path must be absolute, got %q", path)
	}
	info, err := os.Stat(path)
	if err != nil {
		return fmt.Errorf("bridge binary not found: %w", err)
	}
	// Reject world-writable binaries (unix permission bit 0o002).
	if info.Mode().Perm()&0o002 != 0 {
		return fmt.Errorf("bridge binary %q is world-writable, refusing to execute", path)
	}
	return nil
}

func (c Client) SafeExport(ctx context.Context, databasePath, destinationPath string) (Response, error) {
	return c.Execute(ctx, Request{
		DatabasePath:    databasePath,
		Command:         CommandSafeExport,
		DestinationPath: destinationPath,
	})
}

func (c Client) Execute(ctx context.Context, request Request) (Response, error) {
	// Security fix H-3: Validate the bridge binary path before execution.
	if err := validateBinaryPath(c.BinaryPath); err != nil {
		return Response{}, err
	}

	body, err := json.Marshal(request)
	if err != nil {
		return Response{}, fmt.Errorf("marshal request: %w", err)
	}

	cmd := exec.CommandContext(ctx, c.BinaryPath)
	cmd.Stdin = bytes.NewReader(body)

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return Response{}, fmt.Errorf("run bridge: %w: %s", err, stderr.String())
	}

	var response Response
	if err := json.Unmarshal(stdout.Bytes(), &response); err != nil {
		return Response{}, fmt.Errorf("decode bridge response: %w", err)
	}
	if response.ProtocolVersion != ProtocolVersion {
		return Response{}, fmt.Errorf(
			"bridge protocol version mismatch: expected %d, got %d",
			ProtocolVersion,
			response.ProtocolVersion,
		)
	}
	return response, nil
}
