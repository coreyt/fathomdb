package bridge

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func writeShellBridge(t *testing.T, script string) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell-backed bridge tests are unix-only")
	}
	binaryPath := filepath.Join(t.TempDir(), "bridge.sh")
	require.NoError(t, os.WriteFile(binaryPath, []byte(script), 0o755))
	return binaryPath
}

func TestRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandRegenerateVectors,
		ConfigPath:   "/tmp/vector-regen.toml",
		VectorGeneratorPolicy: &VectorGeneratorPolicy{
			TimeoutMS:                     1234,
			MaxStdoutBytes:                2048,
			MaxStderrBytes:                1024,
			MaxInputBytes:                 4096,
			MaxChunks:                     77,
			RequireAbsoluteExecutable:     true,
			RejectWorldWritableExecutable: true,
			AllowedExecutableRoots:        []string{"/usr/local/bin"},
			PreserveEnvVars:               []string{"OPENAI_API_KEY"},
		},
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"protocol_version":1`)
	require.Contains(t, string(body), `"database_path":"/tmp/fathom.db"`)
	require.Contains(t, string(body), `"command":"regenerate_vector_embeddings"`)
	require.Contains(t, string(body), `"config_path":"/tmp/vector-regen.toml"`)
	require.Contains(t, string(body), `"vector_generator_policy"`)
	require.Contains(t, string(body), `"timeout_ms":1234`)
	require.Contains(t, string(body), `"max_chunks":77`)
	require.Contains(t, string(body), `"require_absolute_executable":true`)
	require.Contains(t, string(body), `"reject_world_writable_executable":true`)
	require.Contains(t, string(body), `"allowed_executable_roots":["/usr/local/bin"]`)
	require.Contains(t, string(body), `"preserve_env_vars":["OPENAI_API_KEY"]`)
}

func TestOperationalRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:   "/tmp/fathom.db",
		Command:        CommandRegisterOperationalCollection,
		CollectionName: "connector_health",
		OperationalCollection: &OperationalCollection{
			Name:          "connector_health",
			Kind:          "latest_state",
			SchemaJSON:    "{}",
			RetentionJSON: "{}",
			FormatVersion: 1,
		},
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"register_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"connector_health"`)
	require.Contains(t, string(body), `"operational_collection"`)
	require.Contains(t, string(body), `"kind":"latest_state"`)
	require.Contains(t, string(body), `"schema_json":"{}"`)
	require.Contains(t, string(body), `"retention_json":"{}"`)
	require.Contains(t, string(body), `"format_version":1`)
}

func TestOperationalLifecycleRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:    "/tmp/fathom.db",
		Command:         CommandPurgeOperationalCollection,
		CollectionName:  "audit_log",
		BeforeTimestamp: 250,
		DryRun:          true,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"purge_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"before_timestamp":250`)
	require.Contains(t, string(body), `"dry_run":true`)
}

func TestLogicalLifecycleRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandRestoreLogicalID,
		LogicalID:    "doc-1",
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"restore_logical_id"`)
	require.Contains(t, string(body), `"logical_id":"doc-1"`)
}

func TestClientRejectsProtocolMismatch(t *testing.T) {
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":99,"ok":true,"message":"ok","payload":{}}'
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	_, err := client.Execute(context.Background(), Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandTraceSource,
		SourceRef:    "source-1",
	})

	require.Error(t, err)
	require.Contains(t, err.Error(), "bridge protocol version mismatch")
}

func TestErrorFromResponseReturnsBridgeErrorWithExitCode(t *testing.T) {
	err := ErrorFromResponse(Response{
		ProtocolVersion: ProtocolVersion,
		OK:              false,
		Message:         "missing source_ref",
		ErrorCode:       ErrorBadRequest,
	})
	require.Error(t, err)

	var bridgeErr BridgeError
	require.ErrorAs(t, err, &bridgeErr)
	require.Equal(t, 2, bridgeErr.ExitCode())
}

func TestExitCodeFromErrorDefaultsToOneForNonBridgeErrors(t *testing.T) {
	require.Equal(t, 1, ExitCodeFromError(os.ErrInvalid))
}

func TestExecuteWithFeedbackEmitsLifecycleEvents(t *testing.T) {
	script := `#!/usr/bin/env bash
sleep 0.05
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"ok","payload":{}}'
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	var events []ResponseCycleEvent

	response, err := client.ExecuteWithFeedback(
		context.Background(),
		Request{
			DatabasePath: "/tmp/fathom.db",
			Command:      CommandTraceSource,
			SourceRef:    "source-1",
		},
		ObserverFunc(func(event ResponseCycleEvent) {
			events = append(events, event)
		}),
		FeedbackConfig{
			SlowThreshold:     5 * time.Millisecond,
			HeartbeatInterval: 10 * time.Millisecond,
		},
	)

	require.NoError(t, err)
	require.True(t, response.OK)
	require.NotEmpty(t, events)
	require.Equal(t, PhaseStarted, events[0].Phase)
	require.Contains(t, phases(events), PhaseSlow)
	require.Contains(t, phases(events), PhaseHeartbeat)
	require.Equal(t, PhaseFinished, events[len(events)-1].Phase)
}

func TestRegenerateVectorsWithFeedbackEmitsLifecycleEvents(t *testing.T) {
	script := `#!/usr/bin/env bash
sleep 0.05
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"vector embeddings regenerated","payload":{"profile":"default","table_name":"vec_nodes_active","dimension":4,"total_chunks":1,"regenerated_rows":1,"contract_persisted":true,"notes":["ok"]}}'
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	var events []ResponseCycleEvent

	response, err := client.RegenerateVectorsWithFeedback(
		context.Background(),
		"/tmp/fathom.db",
		"/tmp/vector-regen.toml",
		ObserverFunc(func(event ResponseCycleEvent) {
			events = append(events, event)
		}),
		FeedbackConfig{
			SlowThreshold:     5 * time.Millisecond,
			HeartbeatInterval: 10 * time.Millisecond,
		},
	)

	require.NoError(t, err)
	require.True(t, response.OK)
	require.NotEmpty(t, events)
	require.Equal(t, PhaseStarted, events[0].Phase)
	require.Contains(t, phases(events), PhaseSlow)
	require.Equal(t, PhaseFinished, events[len(events)-1].Phase)
}

func phases(events []ResponseCycleEvent) []ResponseCyclePhase {
	result := make([]ResponseCyclePhase, 0, len(events))
	for _, event := range events {
		result = append(result, event.Phase)
	}
	return result
}
