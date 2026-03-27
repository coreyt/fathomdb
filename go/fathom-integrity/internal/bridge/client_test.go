package bridge

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

func TestRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandRegenerateVectors,
		ConfigPath:   "/tmp/vector-regen.toml",
		VectorGeneratorPolicy: &VectorGeneratorPolicy{
			TimeoutMS:          1234,
			MaxStdoutBytes:     2048,
			MaxStderrBytes:     1024,
			MaxInputBytes:      4096,
			MaxChunks:          77,
			WarnExecutablePath: true,
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
}

func TestClientRejectsProtocolMismatch(t *testing.T) {
	binaryPath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":99,"ok":true,"message":"ok","payload":{}}'
`
	require.NoError(t, os.WriteFile(binaryPath, []byte(script), 0o755))

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
	binaryPath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
sleep 0.05
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"ok","payload":{}}'
`
	require.NoError(t, os.WriteFile(binaryPath, []byte(script), 0o755))

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
	binaryPath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
sleep 0.05
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"vector embeddings regenerated","payload":{"profile":"default","table_name":"vec_nodes_active","dimension":4,"total_chunks":1,"regenerated_rows":1,"contract_persisted":true,"notes":["ok"]}}'
`
	require.NoError(t, os.WriteFile(binaryPath, []byte(script), 0o755))

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
