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

// ProtocolVersion is the JSON protocol version exchanged between the Go CLI and
// the fathomdb-admin-bridge Rust binary. Both sides must agree on this value.
const ProtocolVersion = 1

// Command identifies a bridge operation sent to the fathomdb-admin-bridge binary.
type Command string

const (
	CommandCheckIntegrity                Command = "check_integrity"
	CommandCheckSemantics                Command = "check_semantics"
	CommandRebuildProjections            Command = "rebuild_projections"
	CommandRebuildMissing                Command = "rebuild_missing_projections"
	CommandRestoreVector                 Command = "restore_vector_profiles"
	CommandRegenerateVectors             Command = "regenerate_vector_embeddings"
	CommandRestoreLogicalID              Command = "restore_logical_id"
	CommandPurgeLogicalID                Command = "purge_logical_id"
	CommandTraceSource                   Command = "trace_source"
	CommandExciseSource                  Command = "excise_source"
	CommandSafeExport                    Command = "safe_export"
	CommandRegisterOperationalCollection Command = "register_operational_collection"
	CommandDescribeOperationalCollection Command = "describe_operational_collection"
	CommandUpdateOperationalFilters      Command = "update_operational_collection_filters"
	CommandUpdateOperationalValidation   Command = "update_operational_collection_validation"
	CommandUpdateOperationalIndexes      Command = "update_operational_collection_secondary_indexes"
	CommandDisableOperationalCollection  Command = "disable_operational_collection"
	CommandCompactOperationalCollection  Command = "compact_operational_collection"
	CommandPurgeOperationalCollection    Command = "purge_operational_collection"
	CommandRebuildOperationalCurrent     Command = "rebuild_operational_current"
	CommandRebuildOperationalIndexes     Command = "rebuild_operational_secondary_indexes"
	CommandTraceOperationalCollection    Command = "trace_operational_collection"
	CommandReadOperationalCollection     Command = "read_operational_collection"
	CommandValidateOperationalHistory    Command = "validate_operational_collection_history"
	CommandPlanOperationalRetention      Command = "plan_operational_retention"
	CommandRunOperationalRetention       Command = "run_operational_retention"
	CommandPurgeProvenanceEvents         Command = "purge_provenance_events"
)

// Request is the JSON envelope sent on stdin to the fathomdb-admin-bridge binary.
type Request struct {
	ProtocolVersion       int                     `json:"protocol_version"`
	DatabasePath          string                  `json:"database_path"`
	Command               Command                 `json:"command"`
	LogicalID             string                  `json:"logical_id,omitempty"`
	Target                string                  `json:"target,omitempty"`
	SourceRef             string                  `json:"source_ref,omitempty"`
	CollectionName        string                  `json:"collection_name,omitempty"`
	CollectionNames       []string                `json:"collection_names,omitempty"`
	RecordKey             string                  `json:"record_key,omitempty"`
	FilterFieldsJSON      string                  `json:"filter_fields_json,omitempty"`
	ValidationJSON        string                  `json:"validation_json"`
	SecondaryIndexesJSON  string                  `json:"secondary_indexes_json,omitempty"`
	NowTimestamp          int64                   `json:"now_timestamp,omitempty"`
	MaxCollections        int                     `json:"max_collections,omitempty"`
	BeforeTimestamp       int64                   `json:"before_timestamp,omitempty"`
	DryRun                bool                    `json:"dry_run,omitempty"`
	PreserveEventTypes    []string                `json:"preserve_event_types,omitempty"`
	DestinationPath       string                  `json:"destination_path,omitempty"`
	ForceCheckpoint       *bool                   `json:"force_checkpoint,omitempty"`
	ConfigPath            string                  `json:"config_path,omitempty"`
	VectorGeneratorPolicy *VectorGeneratorPolicy  `json:"vector_generator_policy,omitempty"`
	OperationalCollection *OperationalCollection  `json:"operational_collection,omitempty"`
	OperationalRead       *OperationalReadRequest `json:"operational_read,omitempty"`
}

// OperationalCollection describes the schema and configuration for a registered
// operational collection within a fathomdb database.
type OperationalCollection struct {
	Name                 string `json:"name"`
	Kind                 string `json:"kind"`
	SchemaJSON           string `json:"schema_json"`
	RetentionJSON        string `json:"retention_json"`
	FilterFieldsJSON     string `json:"filter_fields_json"`
	ValidationJSON       string `json:"validation_json"`
	SecondaryIndexesJSON string `json:"secondary_indexes_json"`
	FormatVersion        int64  `json:"format_version"`
}

// OperationalFilterClause specifies a single filter predicate for querying an
// operational collection.
type OperationalFilterClause struct {
	Mode  string `json:"mode"`
	Field string `json:"field"`
	Value any    `json:"value,omitempty"`
	Lower *int64 `json:"lower,omitempty"`
	Upper *int64 `json:"upper,omitempty"`
}

// OperationalReadRequest contains the parameters for reading records from an
// operational collection via the bridge.
type OperationalReadRequest struct {
	CollectionName string                    `json:"collection_name"`
	Filters        []OperationalFilterClause `json:"filters"`
	Limit          int                       `json:"limit,omitempty"`
}

// VectorGeneratorPolicy constrains how the Rust engine spawns external vector
// generator executables during embedding regeneration.
type VectorGeneratorPolicy struct {
	TimeoutMS                     uint64   `json:"timeout_ms"`
	MaxStdoutBytes                int      `json:"max_stdout_bytes"`
	MaxStderrBytes                int      `json:"max_stderr_bytes"`
	MaxInputBytes                 int      `json:"max_input_bytes"`
	MaxChunks                     int      `json:"max_chunks"`
	RequireAbsoluteExecutable     bool     `json:"require_absolute_executable"`
	RejectWorldWritableExecutable bool     `json:"reject_world_writable_executable"`
	AllowedExecutableRoots        []string `json:"allowed_executable_roots,omitempty"`
	PreserveEnvVars               []string `json:"preserve_env_vars,omitempty"`
}

// DefaultVectorGeneratorPolicy returns a VectorGeneratorPolicy with conservative
// defaults suitable for production use.
func DefaultVectorGeneratorPolicy() VectorGeneratorPolicy {
	return VectorGeneratorPolicy{
		TimeoutMS:                     300000,
		MaxStdoutBytes:                64 * 1024 * 1024,
		MaxStderrBytes:                1024 * 1024,
		MaxInputBytes:                 64 * 1024 * 1024,
		MaxChunks:                     1000000,
		RequireAbsoluteExecutable:     true,
		RejectWorldWritableExecutable: true,
		AllowedExecutableRoots:        nil,
		PreserveEnvVars:               nil,
	}
}

// Response is the JSON envelope returned on stdout by the fathomdb-admin-bridge binary.
type Response struct {
	ProtocolVersion int             `json:"protocol_version"`
	OK              bool            `json:"ok"`
	Message         string          `json:"message"`
	ErrorCode       string          `json:"error_code,omitempty"`
	Payload         json.RawMessage `json:"payload"`
}

// Error code constants returned in the Response ErrorCode field.
const (
	// ErrorBadRequest indicates the request was malformed or missing required fields.
	ErrorBadRequest = "bad_request"
	// ErrorUnsupportedCommand indicates the bridge does not recognize the command.
	ErrorUnsupportedCommand = "unsupported_command"
	// ErrorUnsupportedCapability indicates the bridge lacks a required capability.
	ErrorUnsupportedCapability = "unsupported_capability"
	// ErrorIntegrityFailure indicates the database failed an integrity check.
	ErrorIntegrityFailure = "integrity_failure"
	// ErrorExecutionFailure indicates a general execution error in the bridge.
	ErrorExecutionFailure = "execution_failure"
)

// Error represents a structured error returned by the fathomdb-admin-bridge
// binary, carrying both an error code and a human-readable message.
type Error struct {
	Code    string
	Message string
}

// Error returns the human-readable error message, implementing the error interface.
func (e Error) Error() string {
	if e.Message == "" {
		return "bridge command failed"
	}
	return e.Message
}

// ExitCode maps the error code to a CLI process exit code.
func (e Error) ExitCode() int {
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

// ErrorFromResponse returns an Error if the response indicates failure, or nil on success.
func ErrorFromResponse(response Response) error {
	if response.OK {
		return nil
	}
	code := response.ErrorCode
	if code == "" {
		code = ErrorExecutionFailure
	}
	return Error{
		Code:    code,
		Message: response.Message,
	}
}

// ExitCodeFromError extracts the CLI exit code from an Error, defaulting to 1.
func ExitCodeFromError(err error) int {
	var bridgeError Error
	if errors.As(err, &bridgeError) {
		return bridgeError.ExitCode()
	}
	return 1
}

// MarshalJSON encodes the request to JSON, injecting ProtocolVersion when unset.
func (r Request) MarshalJSON() ([]byte, error) {
	type alias Request
	payload := alias(r)
	if payload.ProtocolVersion == 0 {
		payload.ProtocolVersion = ProtocolVersion
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return nil, fmt.Errorf("marshal bridge request: %w", err)
	}
	return data, nil
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

// ProvenancePurgeReport is the structured payload returned by the bridge
// purge_provenance_events command.
type ProvenancePurgeReport struct {
	EventsDeleted   int    `json:"events_deleted"`
	EventsPreserved int    `json:"events_preserved"`
	OldestRemaining *int64 `json:"oldest_remaining"`
}

// Client wraps the fathomdb-admin-bridge subprocess for administrative operations.
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

// SafeExport performs a bridge-backed SQLite backup of the database to destinationPath.
func (c Client) SafeExport(ctx context.Context, databasePath, destinationPath string, forceCheckpoint bool) (Response, error) {
	forceCheckpointValue := forceCheckpoint
	return c.Execute(ctx, Request{
		DatabasePath:    databasePath,
		Command:         CommandSafeExport,
		DestinationPath: destinationPath,
		ForceCheckpoint: &forceCheckpointValue,
	})
}

// SafeExportWithFeedback is like SafeExport but emits lifecycle feedback events via the observer.
func (c Client) SafeExportWithFeedback(
	ctx context.Context,
	databasePath, destinationPath string,
	forceCheckpoint bool,
	observer Observer,
	config FeedbackConfig,
) (Response, error) {
	forceCheckpointValue := forceCheckpoint
	return c.ExecuteWithFeedback(ctx, Request{
		DatabasePath:    databasePath,
		Command:         CommandSafeExport,
		DestinationPath: destinationPath,
		ForceCheckpoint: &forceCheckpointValue,
	}, observer, config)
}

// PurgeProvenanceEvents deletes provenance events older than beforeTimestamp,
// preserving any event types listed in preserveEventTypes.
func (c Client) PurgeProvenanceEvents(ctx context.Context, databasePath string, beforeTimestamp int64, preserveEventTypes []string) (*ProvenancePurgeReport, error) {
	resp, err := c.Execute(ctx, Request{
		DatabasePath:       databasePath,
		Command:            CommandPurgeProvenanceEvents,
		BeforeTimestamp:    beforeTimestamp,
		PreserveEventTypes: preserveEventTypes,
	})
	if err != nil {
		return nil, err
	}
	if respErr := ErrorFromResponse(resp); respErr != nil {
		return nil, respErr
	}
	var report ProvenancePurgeReport
	if err := json.Unmarshal(resp.Payload, &report); err != nil {
		return nil, fmt.Errorf("decode provenance purge report: %w", err)
	}
	return &report, nil
}

// RegenerateVectors regenerates vector embeddings for the database using the
// default vector generator policy.
func (c Client) RegenerateVectors(
	ctx context.Context,
	databasePath, configPath string,
) (Response, error) {
	return c.RegenerateVectorsWithPolicy(ctx, databasePath, configPath, nil)
}

// RegenerateVectorsWithPolicy is like RegenerateVectors but accepts an explicit
// VectorGeneratorPolicy to constrain generator subprocess execution.
func (c Client) RegenerateVectorsWithPolicy(
	ctx context.Context,
	databasePath, configPath string,
	policy *VectorGeneratorPolicy,
) (Response, error) {
	return c.Execute(ctx, Request{
		DatabasePath:          databasePath,
		Command:               CommandRegenerateVectors,
		ConfigPath:            configPath,
		VectorGeneratorPolicy: policy,
	})
}

// RegenerateVectorsWithFeedback is like RegenerateVectors but emits lifecycle
// feedback events via the observer.
func (c Client) RegenerateVectorsWithFeedback(
	ctx context.Context,
	databasePath, configPath string,
	observer Observer,
	config FeedbackConfig,
) (Response, error) {
	return c.RegenerateVectorsWithFeedbackAndPolicy(
		ctx,
		databasePath,
		configPath,
		nil,
		observer,
		config,
	)
}

// RegenerateVectorsWithFeedbackAndPolicy is like RegenerateVectorsWithPolicy but
// also emits lifecycle feedback events via the observer.
func (c Client) RegenerateVectorsWithFeedbackAndPolicy(
	ctx context.Context,
	databasePath, configPath string,
	policy *VectorGeneratorPolicy,
	observer Observer,
	config FeedbackConfig,
) (Response, error) {
	return c.ExecuteWithFeedback(ctx, Request{
		DatabasePath:          databasePath,
		Command:               CommandRegenerateVectors,
		ConfigPath:            configPath,
		VectorGeneratorPolicy: policy,
	}, observer, config)
}

// Execute sends a bridge request and returns the parsed response.
//
// The bridge binary is spawned as a subprocess with JSON on stdin/stdout.
func (c Client) Execute(ctx context.Context, request Request) (Response, error) {
	return c.ExecuteWithFeedback(ctx, request, nil, FeedbackConfig{})
}

// ExecuteWithFeedback is like Execute but emits lifecycle feedback events via the observer.
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
	if request.LogicalID != "" {
		metadata["logical_id"] = request.LogicalID
	}
	if request.DestinationPath != "" {
		metadata["destination_path"] = request.DestinationPath
	}
	if request.ConfigPath != "" {
		metadata["config_path"] = request.ConfigPath
	}
	if request.VectorGeneratorPolicy != nil {
		metadata["vector_generator_policy"] = "configured"
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

		cmd := exec.CommandContext(ctx, c.BinaryPath) //nolint:gosec // G204: BinaryPath validated by validateBinaryPath at line 412
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
