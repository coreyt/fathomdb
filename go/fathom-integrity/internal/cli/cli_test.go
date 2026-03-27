package cli

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/stretchr/testify/require"
)

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
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"invalid target","error_code":"bad_request","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 2, exitCode)
	require.Contains(t, stderr.String(), "invalid target")
}

func TestMainRebuildMapsIntegrityFailureToExitCodeFour(t *testing.T) {
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"integrity failed","error_code":"integrity_failure","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 4, exitCode)
	require.Contains(t, stderr.String(), "integrity failed")
}

func TestMainRebuildMapsUnsupportedCapabilityToExitCodeThree(t *testing.T) {
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
printf '%s\n' '{"protocol_version":1,"ok":false,"message":"sqlite-vec unavailable","error_code":"unsupported_capability","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	require.Equal(t, 3, exitCode)
	require.Contains(t, stderr.String(), "sqlite-vec unavailable")
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
	bridgePath := filepath.Join(t.TempDir(), "bridge.sh")
	script := `#!/usr/bin/env bash
sleep 0.6
printf '%s\n' '{"protocol_version":1,"ok":true,"message":"ok","payload":{}}'
`
	require.NoError(t, os.WriteFile(bridgePath, []byte(script), 0o755))

	var stdout bytes.Buffer
	var stderr bytes.Buffer

	start := time.Now()
	exitCode := Main([]string{"rebuild", "--db", "/tmp/fathom.db", "--bridge", bridgePath}, &stdout, &stderr)
	elapsed := time.Since(start)

	require.Equal(t, 0, exitCode)
	require.Contains(t, stderr.String(), "rebuild_projections exceeded 500ms")
	require.Less(t, 500*time.Millisecond, elapsed)
}
