package cli

import (
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

type feedbackObserver struct {
	stderr io.Writer
}

func newFeedbackObserver(stderr io.Writer) bridge.Observer {
	return feedbackObserver{stderr: stderr}
}

func (o feedbackObserver) OnEvent(event bridge.ResponseCycleEvent) {
	switch event.Phase {
	case bridge.PhaseSlow:
		fmt.Fprintf(
			o.stderr,
			"%s exceeded %dms and is still running\n",
			event.OperationKind,
			event.SlowThresholdMS,
		)
	case bridge.PhaseHeartbeat:
		fmt.Fprintf(
			o.stderr,
			"%s still running after %dms\n",
			event.OperationKind,
			event.ElapsedMS,
		)
	case bridge.PhaseStarted, bridge.PhaseFinished, bridge.PhaseFailed:
		// No CLI output needed for lifecycle bookkeeping phases.
	}
}
