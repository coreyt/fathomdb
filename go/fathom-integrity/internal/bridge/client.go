package bridge

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
)

type Request struct {
	DatabasePath    string `json:"database_path"`
	Command         string `json:"command"`
	Target          string `json:"target,omitempty"`
	SourceRef       string `json:"source_ref,omitempty"`
	DestinationPath string `json:"destination_path,omitempty"`
}

type Response struct {
	OK      bool            `json:"ok"`
	Message string          `json:"message"`
	Payload json.RawMessage `json:"payload"`
}

type Client struct {
	BinaryPath string
}

func (c Client) Execute(ctx context.Context, request Request) (Response, error) {
	body, err := json.Marshal(request)
	if err != nil {
		return Response{}, fmt.Errorf("marshal request: %w", err)
	}

	cmd := exec.CommandContext(ctx, c.BinaryPath)
	cmd.Stdin = bytes.NewReader(body)

	var stdout bytes.Buffer
	var stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return Response{}, fmt.Errorf("run bridge: %w: %s", err, stderr.String())
	}

	var response Response
	if err := json.Unmarshal(stdout.Bytes(), &response); err != nil {
		return Response{}, fmt.Errorf("decode bridge response: %w", err)
	}
	return response, nil
}
