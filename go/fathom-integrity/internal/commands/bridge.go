package commands

import (
	"context"
	"errors"
	"fmt"
	"io"
	"time"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

func RunBridgeCommand(client bridge.Client, request bridge.Request, out io.Writer) error {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	response, err := client.Execute(ctx, request)
	if err != nil {
		return err
	}
	if !response.OK {
		return errors.New(response.Message)
	}
	if len(response.Payload) > 0 && string(response.Payload) != "{}" {
		_, err = fmt.Fprintf(out, "%s\n%s\n", response.Message, response.Payload)
		return err
	}
	_, err = fmt.Fprintln(out, response.Message)
	return err
}
