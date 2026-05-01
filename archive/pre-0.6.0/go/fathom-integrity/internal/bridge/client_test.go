package bridge

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
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
	require.NoError(t, os.WriteFile(binaryPath, []byte(script), 0o755)) //nolint:gosec // G306: test executable in t.TempDir()
	return binaryPath
}

func TestSafeExportRequestJSONShapeOmitsForceCheckpointByDefault(t *testing.T) {
	request := Request{
		DatabasePath:    "/tmp/fathom.db",
		Command:         CommandSafeExport,
		DestinationPath: "/tmp/export.db",
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"safe_export"`)
	require.Contains(t, string(body), `"destination_path":"/tmp/export.db"`)
	require.NotContains(t, string(body), `"force_checkpoint"`)
}

func TestSafeExportRequestJSONShapeIncludesExplicitFalseForceCheckpoint(t *testing.T) {
	forceCheckpoint := false
	request := Request{
		DatabasePath:    "/tmp/fathom.db",
		Command:         CommandSafeExport,
		DestinationPath: "/tmp/export.db",
		ForceCheckpoint: &forceCheckpoint,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"safe_export"`)
	require.Contains(t, string(body), `"force_checkpoint":false`)
}

func TestSafeExportRequestJSONShapeIncludesForceCheckpointWhenRequested(t *testing.T) {
	forceCheckpoint := true
	request := Request{
		DatabasePath:    "/tmp/fathom.db",
		Command:         CommandSafeExport,
		DestinationPath: "/tmp/export.db",
		ForceCheckpoint: &forceCheckpoint,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"safe_export"`)
	require.Contains(t, string(body), `"force_checkpoint":true`)
}

func TestOperationalRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:   "/tmp/fathom.db",
		Command:        CommandRegisterOperationalCollection,
		CollectionName: "connector_health",
		OperationalCollection: &OperationalCollection{
			Name:             "connector_health",
			Kind:             "latest_state",
			SchemaJSON:       "{}",
			RetentionJSON:    "{}",
			FilterFieldsJSON: "[]",
			FormatVersion:    1,
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
	require.Contains(t, string(body), `"filter_fields_json":"[]"`)
	require.Contains(t, string(body), `"secondary_indexes_json":""`)
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

func TestOperationalFilterUpdateRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:     "/tmp/fathom.db",
		Command:          CommandUpdateOperationalFilters,
		CollectionName:   "audit_log",
		FilterFieldsJSON: `[{"name":"actor","type":"string","modes":["exact"]}]`,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"update_operational_collection_filters"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"filter_fields_json":"[{\"name\":\"actor\",\"type\":\"string\",\"modes\":[\"exact\"]}]"`)
}

func TestOperationalValidationUpdateRequestPreservesEmptyString(t *testing.T) {
	request := Request{
		DatabasePath:   "/tmp/fathom.db",
		Command:        CommandUpdateOperationalValidation,
		CollectionName: "audit_log",
		ValidationJSON: "",
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"update_operational_collection_validation"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"validation_json":""`)
}

func TestOperationalReadRequestJSONShape(t *testing.T) {
	lower := int64(150)
	upper := int64(250)
	request := Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandReadOperationalCollection,
		OperationalRead: &OperationalReadRequest{
			CollectionName: "audit_log",
			Filters: []OperationalFilterClause{
				{
					Mode:  "prefix",
					Field: "actor",
					Value: "alice",
				},
				{
					Mode:  "range",
					Field: "ts",
					Lower: &lower,
					Upper: &upper,
				},
			},
			Limit: 10,
		},
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"read_operational_collection"`)
	require.Contains(t, string(body), `"operational_read"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"mode":"prefix"`)
	require.Contains(t, string(body), `"field":"actor"`)
	require.Contains(t, string(body), `"value":"alice"`)
	require.Contains(t, string(body), `"mode":"range"`)
	require.Contains(t, string(body), `"lower":150`)
	require.Contains(t, string(body), `"upper":250`)
	require.Contains(t, string(body), `"limit":10`)
}

func TestOperationalReadRequestJSONShapePreservesZeroRangeBounds(t *testing.T) {
	zero := int64(0)
	request := Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandReadOperationalCollection,
		OperationalRead: &OperationalReadRequest{
			CollectionName: "audit_log",
			Filters: []OperationalFilterClause{
				{
					Mode:  "range",
					Field: "ts",
					Lower: &zero,
					Upper: &zero,
				},
			},
		},
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"lower":0`)
	require.Contains(t, string(body), `"upper":0`)
}

func TestOperationalSecondaryIndexUpdateRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:         "/tmp/fathom.db",
		Command:              CommandUpdateOperationalIndexes,
		CollectionName:       "audit_log",
		SecondaryIndexesJSON: `[{"name":"actor_ts","kind":"append_only_field_time","field":"actor","value_type":"string","time_field":"ts"}]`,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"update_operational_collection_secondary_indexes"`)
	require.Contains(t, string(body), `"secondary_indexes_json":"[{\"name\":\"actor_ts\",\"kind\":\"append_only_field_time\",\"field\":\"actor\",\"value_type\":\"string\",\"time_field\":\"ts\"}]"`)
}

func TestOperationalRetentionRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:    "/tmp/fathom.db",
		Command:         CommandRunOperationalRetention,
		CollectionNames: []string{"audit_log"},
		NowTimestamp:    1000,
		MaxCollections:  5,
		DryRun:          true,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"run_operational_retention"`)
	require.Contains(t, string(body), `"collection_names":["audit_log"]`)
	require.Contains(t, string(body), `"now_timestamp":1000`)
	require.Contains(t, string(body), `"max_collections":5`)
	require.Contains(t, string(body), `"dry_run":true`)
}

func TestPurgeProvenanceEventsRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath:    "/tmp/fathom.db",
		Command:         CommandPurgeProvenanceEvents,
		BeforeTimestamp: 1700000000,
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"protocol_version":1`)
	require.Contains(t, string(body), `"command":"purge_provenance_events"`)
	require.Contains(t, string(body), `"before_timestamp":1700000000`)
	require.NotContains(t, string(body), `"preserve_event_types"`)
}

func TestPurgeProvenanceEventsRequestWithPreserveTypes(t *testing.T) {
	request := Request{
		DatabasePath:       "/tmp/fathom.db",
		Command:            CommandPurgeProvenanceEvents,
		BeforeTimestamp:    1700000000,
		PreserveEventTypes: []string{"excise", "restore"},
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"purge_provenance_events"`)
	require.Contains(t, string(body), `"before_timestamp":1700000000`)
	require.Contains(t, string(body), `"preserve_event_types":["excise","restore"]`)
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

func TestErrorFromResponseReturnsErrorWithExitCode(t *testing.T) {
	err := ErrorFromResponse(Response{
		ProtocolVersion: ProtocolVersion,
		OK:              false,
		Message:         "missing source_ref",
		ErrorCode:       ErrorBadRequest,
	})
	require.Error(t, err)

	var bridgeErr Error
	require.ErrorAs(t, err, &bridgeErr)
	require.Equal(t, 2, bridgeErr.ExitCode())
}

func TestExitCodeFromErrorDefaultsToOneForNonErrors(t *testing.T) {
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

func phases(events []ResponseCycleEvent) []ResponseCyclePhase {
	result := make([]ResponseCyclePhase, 0, len(events))
	for _, event := range events {
		result = append(result, event.Phase)
	}
	return result
}

func TestStderrIncludedOnOkFalse(t *testing.T) {
	script := `#!/usr/bin/env bash
echo "rust: diagnostics from engine" >&2
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"integrity check failed","error_code":"integrity_failure","payload":{}}'
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	resp, err := client.Execute(context.Background(), Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandCheckIntegrity,
	})

	require.NoError(t, err)
	require.False(t, resp.OK)
	require.Contains(t, resp.Message, "integrity check failed")
	require.Contains(t, resp.Message, "stderr:")
	require.Contains(t, resp.Message, "rust: diagnostics from engine")
}

func TestStderrNotIncludedOnOkTrue(t *testing.T) {
	script := `#!/usr/bin/env bash
echo "some debug info" >&2
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"all good","payload":{}}'
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	resp, err := client.Execute(context.Background(), Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandCheckIntegrity,
	})

	require.NoError(t, err)
	require.True(t, resp.OK)
	require.NotContains(t, resp.Message, "stderr:")
}

func TestStderrOnOkFalseWithEmptyMessage(t *testing.T) {
	script := `#!/usr/bin/env bash
echo "engine context" >&2
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"","error_code":"execution_failure","payload":{}}'
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	resp, err := client.Execute(context.Background(), Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandCheckIntegrity,
	})

	require.NoError(t, err)
	require.False(t, resp.OK)
	require.Contains(t, resp.Message, "stderr: engine context")
}

func TestStdoutLimitTruncatesLargeOutput(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping large-output test in short mode")
	}
	// Use dd to quickly generate 65 MB of null bytes on stdout.
	// The output will be truncated to 64 MB and will not be valid JSON.
	script := `#!/usr/bin/env bash
dd if=/dev/zero bs=1048576 count=65 2>/dev/null
`
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	_, err := client.Execute(context.Background(), Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandCheckIntegrity,
	})

	// The output is truncated so it will not be valid JSON.
	require.Error(t, err)
	require.Contains(t, err.Error(), "decode bridge response")
}

func TestStderrLimitTruncatesLargeStderr(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping large-output test in short mode")
	}
	// Use dd to quickly generate 2 MB of data on stderr.
	// The stderr should be truncated to 1 MB.
	script := fmt.Sprintf(`#!/usr/bin/env bash
# Write 2 MB of zeroes to fd 3, then redirect fd 3 to stderr
exec 3>&2
dd if=/dev/zero bs=1048576 count=2 >&3 2>/dev/null
exec 3>&-
printf '%%s\n' '{"protocol_version":1,"ok":false,"message":"fail","error_code":"execution_failure","payload":{}}'
`)
	binaryPath := writeShellBridge(t, script)

	client := Client{BinaryPath: binaryPath}
	resp, err := client.Execute(context.Background(), Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      CommandCheckIntegrity,
	})

	require.NoError(t, err)
	require.False(t, resp.OK)
	require.Contains(t, resp.Message, "stderr:")
	// The stderr content should be truncated to at most 1 MB
	stderrIdx := strings.Index(resp.Message, "stderr: ")
	require.Greater(t, stderrIdx, -1)
	stderrContent := resp.Message[stderrIdx+len("stderr: "):]
	require.LessOrEqual(t, len(stderrContent), 1*1024*1024)
}
