package cli

import (
	"bytes"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/stretchr/testify/require"
)

func writeShellBridge(t *testing.T, script string) string {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("shell-backed bridge tests are unix-only")
	}
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))
	return bridgePath
}

func TestMainRequiresCommand(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main(nil, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "usage:")
}

func TestMainRecoverRequiresDBAndDest(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"recover"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db and --dest are required")
}

func TestMainRepairRequiresDB(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"repair"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db is required")
}

func TestMainTraceOperationalRequiresDBAndCollection(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"trace-operational"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db and --collection are required")
}

func TestMainReadOperationalRequiresDBCollectionAndFilters(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"read-operational"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db, --collection, and --filters-json are required")
}

func TestMainUpdateOperationalFiltersRequiresDBCollectionAndFilterFields(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"update-operational-filters"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db, --collection, and --filter-fields-json are required")
}

func TestMainRestoreLogicalIDRequiresDBAndLogicalID(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"restore-logical-id"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db and --logical-id are required")
}

func TestMainPurgeLogicalIDRequiresDBAndLogicalID(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{"purge-logical-id"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db and --logical-id are required")
}

func TestMainVersionCommand(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"version"}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	require.Contains(t, stdout.String(), "fathom-integrity 0.1.0")
	require.Contains(t, stdout.String(), "admin protocol 1")
	require.Empty(t, stderr.String())
}

func TestMainRebuildMapsBadRequestToUsageExitCode(t *testing.T) {
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"invalid target","error_code":"bad_request","payload":{}}'
`
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "invalid target")
}

func TestMainRebuildMapsIntegrityFailureToExitCodeFour(t *testing.T) {
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"integrity failed","error_code":"integrity_failure","payload":{}}'
`
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 4, exitCode)
	require.Contains(t, stderr.String(), "integrity failed")
}

func TestMainRebuildMapsUnsupportedCapabilityToExitCodeThree(t *testing.T) {
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"sqlite-vec unavailable","error_code":"unsupported_capability","payload":{}}'
`
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 3, exitCode)
	require.Contains(t, stderr.String(), "sqlite-vec unavailable")
}

func TestMainRepairMapsInvalidTargetToUsageExitCode(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"repair", "--db", "/tmp/fathom.db", "--target", "weird"}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "invalid repair target")
}

func TestMainRegenerateVectorsRejectsNonPositiveGeneratorLimits(t *testing.T) {
	var stdout, stderr bytes.Buffer

	exitCode := Main([]string{
		"regenerate-vectors",
		"--db", "/tmp/fathom.db",
		"--config", "/tmp/vector.toml",
		"--generator-max-chunks", "0",
	}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "generator limits must be greater than zero")
}

func TestMainRegenerateVectorsForwardsGeneratorPolicy(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"regenerate-vectors",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--config", "/tmp/vector.toml",
		"--generator-timeout-ms", "1234",
		"--generator-max-stdout-bytes", "2222",
		"--generator-max-stderr-bytes", "3333",
		"--generator-max-input-bytes", "4444",
		"--generator-max-chunks", "55",
		"--generator-allowed-root", "/usr/local/bin",
		"--generator-preserve-env", "OPENAI_API_KEY",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"vector_generator_policy"`)
	require.Contains(t, string(body), `"timeout_ms":1234`)
	require.Contains(t, string(body), `"max_stdout_bytes":2222`)
	require.Contains(t, string(body), `"max_stderr_bytes":3333`)
	require.Contains(t, string(body), `"max_input_bytes":4444`)
	require.Contains(t, string(body), `"max_chunks":55`)
	require.Contains(t, string(body), `"require_absolute_executable":true`)
	require.Contains(t, string(body), `"reject_world_writable_executable":true`)
	require.Contains(t, string(body), `"allowed_executable_roots":["/usr/local/bin"]`)
	require.Contains(t, string(body), `"preserve_env_vars":["OPENAI_API_KEY"]`)
}

func TestMainRegenerateVectorsDisplaysRetryableFailureMessage(t *testing.T) {
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"vector regeneration snapshot drift: chunk snapshot changed during generation; retry [retryable]","error_code":"execution_failure","payload":{}}'
`
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"regenerate-vectors",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--config", "/tmp/vector.toml",
	}, &stdout, &stderr)

	require.Equal(t, 1, exitCode)
	require.Contains(t, stderr.String(), "snapshot drift")
	require.Contains(t, stderr.String(), "[retryable]")
}

func TestMainTraceOperationalForwardsCollectionAndRecordKey(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"trace-operational",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "connector_health",
		"--record-key", "gmail",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"trace_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"connector_health"`)
	require.Contains(t, string(body), `"record_key":"gmail"`)
}

func TestMainReadOperationalForwardsStructuredFilters(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"read-operational",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "audit_log",
		"--filters-json", `[{"mode":"prefix","field":"actor","value":"alice"},{"mode":"range","field":"ts","lower":150,"upper":250}]`,
		"--limit", "10",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"read_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"operational_read"`)
	require.Contains(t, string(body), `"mode":"prefix"`)
	require.Contains(t, string(body), `"mode":"range"`)
	require.Contains(t, string(body), `"limit":10`)
}

func TestMainReadOperationalPreservesZeroRangeBounds(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"read-operational",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "audit_log",
		"--filters-json", `[{"mode":"range","field":"ts","lower":0,"upper":0}]`,
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"lower":0`)
	require.Contains(t, string(body), `"upper":0`)
}

func TestMainUpdateOperationalFiltersForwardsContract(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"update-operational-filters",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "audit_log",
		"--filter-fields-json", `[{"name":"actor","type":"string","modes":["exact"]}]`,
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"update_operational_collection_filters"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"filter_fields_json":"[{\"name\":\"actor\",\"type\":\"string\",\"modes\":[\"exact\"]}]"`)
}

func TestMainRestoreLogicalIDForwardsLogicalID(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"restore-logical-id",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--logical-id", "doc-1",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"restore_logical_id"`)
	require.Contains(t, string(body), `"logical_id":"doc-1"`)
}

func TestMainPurgeLogicalIDForwardsLogicalID(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"purge-logical-id",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--logical-id", "doc-1",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"purge_logical_id"`)
	require.Contains(t, string(body), `"logical_id":"doc-1"`)
}

func TestMainRebuildOperationalCurrentForwardsCollection(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"rebuild-operational-current",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "connector_health",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"rebuild_operational_current"`)
	require.Contains(t, string(body), `"collection_name":"connector_health"`)
}

func TestMainDisableOperationalForwardsCollection(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"disable-operational",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "audit_log",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"disable_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
}

func TestMainCompactOperationalForwardsCollectionAndDryRun(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"compact-operational",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "audit_log",
		"--dry-run",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"compact_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"dry_run":true`)
}

func TestMainPurgeOperationalRequiresBefore(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"purge-operational",
		"--db", "/tmp/fathom.db",
		"--collection", "audit_log",
	}, &stdout, &stderr)

	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "--db, --collection, and --before are required")
}

func TestMainPurgeOperationalForwardsCollectionAndBefore(t *testing.T) {
	tempDir := t.TempDir()
	requestPath := filepath.Join(tempDir, "request.json")
	script := "#!/usr/bin/env bash\ncat >" + requestPath + "\nprintf '%s\\n' '{\"protocol_version\":1,\"ok\":true,\"message\":\"ok\",\"payload\":{}}'\n"
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{
		"purge-operational",
		"--db", "/tmp/fathom.db",
		"--bridge", bridgePath,
		"--collection", "audit_log",
		"--before", "250",
	}, &stdout, &stderr)

	require.Equal(t, 0, exitCode)
	body, err := os.ReadFile(requestPath)
	require.NoError(t, err)
	require.Contains(t, string(body), `"command":"purge_operational_collection"`)
	require.Contains(t, string(body), `"collection_name":"audit_log"`)
	require.Contains(t, string(body), `"before_timestamp":250`)
}

func TestFeedbackObserverWritesSlowAndHeartbeatMessages(t *testing.T) {
	var stderr bytes.Buffer
	observer := newFeedbackObserver(&stderr)

	observer.OnEvent(bridge.ResponseCycleEvent{
		OperationKind:   "rebuild_projections",
		Phase:           bridge.PhaseStarted,
		SlowThresholdMS: 500,
	})
	observer.OnEvent(bridge.ResponseCycleEvent{
		OperationKind:   "rebuild_projections",
		Phase:           bridge.PhaseSlow,
		ElapsedMS:       600,
		SlowThresholdMS: 500,
	})
	observer.OnEvent(bridge.ResponseCycleEvent{
		OperationKind: "rebuild_projections",
		Phase:         bridge.PhaseHeartbeat,
		ElapsedMS:     2600,
	})
	observer.OnEvent(bridge.ResponseCycleEvent{
		OperationKind: "rebuild_projections",
		Phase:         bridge.PhaseFinished,
		ElapsedMS:     2650,
	})

	output := stderr.String()
	require.Contains(t, output, "rebuild_projections exceeded 500ms")
	require.Contains(t, output, "rebuild_projections still running after 2600ms")
}

func TestMainRebuildEmitsSlowFeedbackOnStderr(t *testing.T) {
	script := `#!/usr/bin/env bash
sleep 0.6
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"ok","payload":{}}'
`
	bridgePath := writeShellBridge(t, script)

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	start := time.Now()
	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	elapsed := time.Since(start)

	require.Equal(t, 0, exitCode)
	require.Contains(t, stderr.String(), "rebuild_projections exceeded 500ms")
	require.Less(t, 500*time.Millisecond, elapsed)
}
