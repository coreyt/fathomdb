package commands

import (
	"context"
	"fmt"
	"io"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/bridge"
)

// RunRegenerateVectors regenerates vector embeddings using the admin bridge and
// a caller-supplied vector contract config file.
func RunRegenerateVectors(databasePath, bridgePath, configPath string, out io.Writer) error {
	return RunRegenerateVectorsWithFeedback(
		databasePath,
		bridgePath,
		configPath,
		nil,
		out,
		nil,
		bridge.FeedbackConfig{},
	)
}

func RunRegenerateVectorsWithFeedback(
	databasePath, bridgePath, configPath string,
	policy *bridge.VectorGeneratorPolicy,
	out io.Writer,
	observer bridge.Observer,
	config bridge.FeedbackConfig,
) error {
	_, err := bridge.RunWithFeedback(
		context.Background(),
		"go",
		"regenerate-vectors",
		map[string]string{
			"database_path": databasePath,
			"config_path":   configPath,
		},
		observer,
		config,
		func(ctx context.Context) (struct{}, error) {
			if bridgePath == "" {
				return struct{}{}, fmt.Errorf(
					"vector regeneration requires the admin bridge binary (--bridge)")
			}
			if configPath == "" {
				return struct{}{}, fmt.Errorf("vector regeneration requires --config")
			}

			client := bridge.Client{BinaryPath: bridgePath}
			resp, err := client.RegenerateVectorsWithPolicy(ctx, databasePath, configPath, policy)
			if err != nil {
				return struct{}{}, fmt.Errorf("regenerate vectors bridge call failed: %w", err)
			}
			if err := bridge.ErrorFromResponse(resp); err != nil {
				return struct{}{}, fmt.Errorf("regenerate vectors failed: %w", err)
			}

			if len(resp.Payload) > 0 && string(resp.Payload) != "{}" {
				_, err = fmt.Fprintf(out, "%s\n%s\n", resp.Message, resp.Payload)
				return struct{}{}, err
			}
			_, err = fmt.Fprintln(out, resp.Message)
			return struct{}{}, err
		},
	)
	return err
}
