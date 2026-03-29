package commands

import (
	"context"
	"fmt"
	"io"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

// RunBridgeCommand executes a bridge request and writes the response to out.
func RunBridgeCommand(client bridge.Client, request bridge.Request, out io.Writer) error {
	return RunBridgeCommandWithFeedback(client, request, out, nil, bridge.FeedbackConfig{})
}

// RunBridgeCommandWithFeedback is like RunBridgeCommand but emits lifecycle feedback
// events via the observer.
func RunBridgeCommandWithFeedback(
	client bridge.Client,
	request bridge.Request,
	out io.Writer,
	observer bridge.Observer,
	config bridge.FeedbackConfig,
) error {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	response, err := client.ExecuteWithFeedback(ctx, request, observer, config)
	if err != nil {
		return err
	}
	if err := bridge.ErrorFromResponse(response); err != nil {
		return err
	}
	if len(response.Payload) > 0 && string(response.Payload) != "{}" {
		_, err = fmt.Fprintf(out, "%s\n%s\n", response.Message, response.Payload)
		return err
	}
	_, err = fmt.Fprintln(out, response.Message)
	return err
}
