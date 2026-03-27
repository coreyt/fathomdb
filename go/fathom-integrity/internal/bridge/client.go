package bridge

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
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
	CommandRestoreVector      Command = "restore_vector_profiles"
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
	ErrorCode       string          `json:"error_code,omitempty"`
	Payload         json.RawMessage `json:"payload"`
}

const (
	ErrorBadRequest            = "bad_request"
	ErrorUnsupportedCommand    = "unsupported_command"
	ErrorUnsupportedCapability = "unsupported_capability"
	ErrorIntegrityFailure      = "integrity_failure"
	ErrorExecutionFailure      = "execution_failure"
)

type BridgeError struct {
	Code    string
	Message string
}

func (e BridgeError) Error() string {
	if e.Message == "" {
		return "bridge command failed"
	}
	return e.Message
}

func (e BridgeError) ExitCode() int {
	switch e.Code {
	case ErrorBadRequest, ErrorUnsupportedCommand:
		return 2
	case ErrorUnsupportedCapability:
		return 3
	case ErrorIntegrityFailure:
		return 4
	default:
		return 1
	}
}

func ErrorFromResponse(response Response) error {
	if response.OK {
		return nil
	}
	code := response.ErrorCode
	if code == "" {
		code = ErrorExecutionFailure
	}
	return BridgeError{
		Code:    code,
		Message: response.Message,
	}
}

func ExitCodeFromError(err error) int {
	var bridgeError BridgeError
	if errors.As(err, &bridgeError) {
		return bridgeError.ExitCode()
	}
	return 1
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

func (c Client) SafeExportWithFeedback(
	ctx context.Context,
	databasePath, destinationPath string,
	observer Observer,
	config FeedbackConfig,
) (Response, error) {
	return c.ExecuteWithFeedback(ctx, Request{
		DatabasePath:    databasePath,
		Command:         CommandSafeExport,
		DestinationPath: destinationPath,
	}, observer, config)
}

func (c Client) Execute(ctx context.Context, request Request) (Response, error) {
	return c.ExecuteWithFeedback(ctx, request, nil, FeedbackConfig{})
}

func (c Client) ExecuteWithFeedback(
	ctx context.Context,
	request Request,
	observer Observer,
	config FeedbackConfig,
) (Response, error) {
	metadata := map[string]string{
		"command": string(request.Command),
	}
	if request.Target != "" {
		metadata["target"] = request.Target
	}
	if request.SourceRef != "" {
		metadata["source_ref"] = request.SourceRef
	}
	if request.DestinationPath != "" {
		metadata["destination_path"] = request.DestinationPath
	}
	return RunWithFeedback(ctx, "go", string(request.Command), metadata, observer, config, func(context.Context) (Response, error) {
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

		return decodeResponse(stdout.Bytes())
	})
}

func decodeResponse(body []byte) (Response, error) {
	var response Response
	if err := json.Unmarshal(body, &response); err != nil {
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
