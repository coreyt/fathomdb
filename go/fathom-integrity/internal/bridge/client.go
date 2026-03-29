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
)

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
	DestinationPath       string                  `json:"destination_path,omitempty"`
	ConfigPath            string                  `json:"config_path,omitempty"`
	VectorGeneratorPolicy *VectorGeneratorPolicy  `json:"vector_generator_policy,omitempty"`
	OperationalCollection *OperationalCollection  `json:"operational_collection,omitempty"`
	OperationalRead       *OperationalReadRequest `json:"operational_read,omitempty"`
}

type OperationalCollection struct {
	Name             string `json:"name"`
	Kind             string `json:"kind"`
	SchemaJSON       string `json:"schema_json"`
	RetentionJSON    string `json:"retention_json"`
	FilterFieldsJSON string `json:"filter_fields_json"`
	ValidationJSON   string `json:"validation_json"`
	SecondaryIndexesJSON string `json:"secondary_indexes_json"`
	FormatVersion    int64  `json:"format_version"`
}

type OperationalFilterClause struct {
	Mode  string `json:"mode"`
	Field string `json:"field"`
	Value any    `json:"value,omitempty"`
	Lower *int64 `json:"lower,omitempty"`
	Upper *int64 `json:"upper,omitempty"`
}

type OperationalReadRequest struct {
	CollectionName string                    `json:"collection_name"`
	Filters        []OperationalFilterClause `json:"filters"`
	Limit          int                       `json:"limit,omitempty"`
}

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

func (c Client) RegenerateVectors(
	ctx context.Context,
	databasePath, configPath string,
) (Response, error) {
	return c.RegenerateVectorsWithPolicy(ctx, databasePath, configPath, nil)
}

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
