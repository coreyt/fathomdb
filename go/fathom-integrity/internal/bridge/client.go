package bridge

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"os/exec"
)

const ProtocolVersion = 1

type Command string

const (
	CommandCheckIntegrity     Command = "check_integrity"
	CommandRebuildProjections Command = "rebuild_projections"
	CommandRebuildMissing     Command = "rebuild_missing_projections"
	CommandTraceSource        Command = "trace_source"
	CommandExciseSource       Command = "excise_source"
	CommandSafeExport         Command = "safe_export"
)

type Request struct {
	ProtocolVersion int     `json:"protocol_version"`
	DatabasePath    string  `json:"database_path"`
	Command         Command `json:"command"`
	Target          string  `json:"target,omitempty"`
	SourceRef       string  `json:"source_ref,omitempty"`
	DestinationPath string  `json:"destination_path,omitempty"`
}

type Response struct {
	ProtocolVersion int             `json:"protocol_version"`
	OK              bool            `json:"ok"`
	Message         string          `json:"message"`
	Payload         json.RawMessage `json:"payload"`
}

func (r Request) MarshalJSON() ([]byte, error) {
	type alias Request
	payload := alias(r)
	if payload.ProtocolVersion == 0 {
		payload.ProtocolVersion = ProtocolVersion
	}
	return json.Marshal(payload)
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
	if response.ProtocolVersion != ProtocolVersion {
		return Response{}, fmt.Errorf(
			"bridge protocol version mismatch: expected %d, got %d",
			ProtocolVersion,
			response.ProtocolVersion,
		)
	}
	return response, nil
}
