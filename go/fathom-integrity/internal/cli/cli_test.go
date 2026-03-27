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
