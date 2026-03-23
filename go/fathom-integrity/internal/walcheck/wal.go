// Package walcheck parses SQLite WAL files to detect truncation and advise on checkpoints.
package walcheck

import (
	"encoding/binary"
	"errors"
	"io"
	"os"
)

const (
	walMagicBE = uint32(0x377f0682) // big-endian checksum WAL
	walMagicLE = uint32(0x377f0683) // little-endian checksum WAL

	walHeaderSize      = 32
	walFrameHeaderSize = 24

	// DefaultCheckpointThreshold is the frame count above which a checkpoint advisory is emitted.
	DefaultCheckpointThreshold = 100
)

// WALReport describes the state of a SQLite WAL file.
type WALReport struct {
	Present          bool   `json:"present"`
	HeaderValid      bool   `json:"header_valid"`
	PageSize         int    `json:"page_size,omitempty"`
	FrameCount       int    `json:"frame_count"`
	Truncated        bool   `json:"truncated"`
	TruncationOffset int64  `json:"truncation_offset,omitempty"`
	CheckpointSeq    uint32 `json:"checkpoint_seq,omitempty"`
	CheckpointNeeded bool   `json:"checkpoint_needed"`
}

// InspectWAL inspects the SQLite WAL file at walPath.
// Returns WALReport{Present: false} without error if the file does not exist.
func InspectWAL(walPath string) (WALReport, error) {
	f, err := os.Open(walPath)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return WALReport{}, nil
		}
		return WALReport{}, err
	}
	defer f.Close()

	report := WALReport{Present: true}

	// Parse the 32-byte WAL header.
	hdr := make([]byte, walHeaderSize)
	if _, err := io.ReadFull(f, hdr); err != nil {
		// File is shorter than a valid header — invalid.
		return report, nil
	}

	magic := binary.BigEndian.Uint32(hdr[0:4])
	var order binary.ByteOrder
	switch magic {
	case walMagicBE:
		order = binary.BigEndian
	case walMagicLE:
		order = binary.LittleEndian
	default:
		return report, nil // invalid magic
	}

	pageSize := int(order.Uint32(hdr[8:12]))
	if pageSize < 512 {
		return report, nil // implausible page size
	}
	checkpointSeq := order.Uint32(hdr[12:16])

	report.HeaderValid = true
	report.PageSize = pageSize
	report.CheckpointSeq = checkpointSeq

	// Count complete frames. Each frame is (walFrameHeaderSize + pageSize) bytes.
	frameSize := int64(walFrameHeaderSize + pageSize)
	frameBuf := make([]byte, frameSize)
	offset := int64(walHeaderSize)

	for {
		n, err := io.ReadFull(f, frameBuf)
		if err == nil {
			// Complete frame read.
			report.FrameCount++
			offset += frameSize
			continue
		}
		if n == 0 {
			// Clean EOF — no partial frame.
			break
		}
		// Partial frame — file is truncated.
		report.Truncated = true
		report.TruncationOffset = offset
		break
	}

	report.CheckpointNeeded = report.FrameCount > DefaultCheckpointThreshold
	return report, nil
}
