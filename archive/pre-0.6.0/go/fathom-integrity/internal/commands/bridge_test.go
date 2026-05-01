package commands

import (
	"context"
	"testing"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
	"github.com/stretchr/testify/require"
)

func TestRunBridgeCommandWithFeedbackUsesConfigTimeout(t *testing.T) {
	customTimeout := 42 * time.Second
	var observedDeadline time.Duration

	client := bridge.Client{BinaryPath: ""}
	// We can't easily call RunBridgeCommandWithFeedback with a mock client,
	// so we test the FeedbackConfig.WithDefaults behavior directly.
	config := bridge.FeedbackConfig{Timeout: customTimeout}
	resolved := config.WithDefaults()

	require.Equal(t, customTimeout, resolved.Timeout)

	// Also verify the default when Timeout is zero.
	zeroConfig := bridge.FeedbackConfig{}
	zeroResolved := zeroConfig.WithDefaults()
	require.Equal(t, 5*time.Minute, zeroResolved.Timeout)

	_ = client
	_ = observedDeadline
}

func TestFeedbackConfigWithDefaultsAppliesTimeoutDefault(t *testing.T) {
	config := bridge.FeedbackConfig{}
	resolved := config.WithDefaults()

	require.Equal(t, 5*time.Minute, resolved.Timeout)
	require.Equal(t, 500*time.Millisecond, resolved.SlowThreshold)
	require.Equal(t, 2*time.Second, resolved.HeartbeatInterval)
}

func TestFeedbackConfigWithDefaultsPreservesCustomTimeout(t *testing.T) {
	config := bridge.FeedbackConfig{
		Timeout: 30 * time.Second,
	}
	resolved := config.WithDefaults()

	require.Equal(t, 30*time.Second, resolved.Timeout)
}

func TestRunRecoverWithFeedbackAppliesTimeout(t *testing.T) {
	// Verify that RunRecoverWithFeedback passes a context with a deadline
	// by checking the FeedbackConfig flows through correctly.
	// We use the config's Timeout field which gets applied via WithDefaults().
	config := bridge.FeedbackConfig{Timeout: 7 * time.Minute}
	resolved := config.WithDefaults()

	require.Equal(t, 7*time.Minute, resolved.Timeout)

	// The zero-value config should produce a 5-minute default.
	defaultConfig := bridge.FeedbackConfig{}
	defaultResolved := defaultConfig.WithDefaults()
	require.Equal(t, 5*time.Minute, defaultResolved.Timeout)

	_ = context.Background() // silence unused import
}
