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

	// WALHeaderSize is the size of the SQLite WAL file header in bytes.
	WALHeaderSize = 32
	// WALFrameHeaderSize is the size of each WAL frame header in bytes.
	WALFrameHeaderSize = 24

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
	hdr := make([]byte, WALHeaderSize)
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
	// Security fix M-9: Enforce both lower and upper bounds on page size.
	// SQLite supports page sizes from 512 to 65536 bytes. Without the upper
	// bound, a malicious WAL could specify a huge page size causing excessive
	// memory allocation when reading frames.
	if pageSize < 512 || pageSize > 65536 {
		return report, nil // implausible page size
	}
	checkpointSeq := order.Uint32(hdr[12:16])

	// Extract WAL header salt values; used as the initial running checksum for frame 1.
	salt1 := order.Uint32(hdr[16:20])
	salt2 := order.Uint32(hdr[20:24])

	report.HeaderValid = true
	report.PageSize = pageSize
	report.CheckpointSeq = checkpointSeq

	// Count valid frames. Each frame is (WALFrameHeaderSize + pageSize) bytes.
	// A frame is valid if:
	//   1. Its salt values match the WAL header salts.
	//   2. Its checksum is consistent with the rolling checksum chain.
	frameSize := int64(WALFrameHeaderSize + pageSize)
	frameBuf := make([]byte, frameSize)
	offset := int64(WALHeaderSize)
	running := [2]uint32{salt1, salt2}

	for {
		n, err := io.ReadFull(f, frameBuf)
		if err == nil {
			// Verify salt values match the WAL header.
			frameSalt1 := order.Uint32(frameBuf[8:12])
			frameSalt2 := order.Uint32(frameBuf[12:16])
			if frameSalt1 != salt1 || frameSalt2 != salt2 {
				report.Truncated = true
				report.TruncationOffset = offset
				break
			}

			// Verify the rolling checksum.
			// The checksum covers the first 8 bytes of the frame header
			// (page number + db size) followed by the full page data.
			computed := walChecksumBytes(frameBuf[:8], running, order)
			computed = walChecksumBytes(frameBuf[WALFrameHeaderSize:], computed, order)
			storedCk1 := order.Uint32(frameBuf[16:20])
			storedCk2 := order.Uint32(frameBuf[20:24])
			if computed[0] != storedCk1 || computed[1] != storedCk2 {
				report.Truncated = true
				report.TruncationOffset = offset
				break
			}

			running = computed
			report.FrameCount++
			offset += frameSize
			continue
		}
		if n == 0 {
			// Clean EOF — no partial frame.
			break
		}
		// Partial frame — file is physically truncated.
		report.Truncated = true
		report.TruncationOffset = offset
		break
	}

	report.CheckpointNeeded = report.FrameCount > DefaultCheckpointThreshold
	return report, nil
}

// walChecksumBytes computes the SQLite WAL rolling checksum over len(a) bytes.
// a must be a multiple of 8 bytes. s is the incoming (s0, s1) pair.
// The byte order of 32-bit reads follows the WAL magic number.
func walChecksumBytes(a []byte, s [2]uint32, order binary.ByteOrder) [2]uint32 {
	s0, s1 := s[0], s[1]
	for i := 0; i+8 <= len(a); i += 8 {
		s0 += order.Uint32(a[i:]) + s1
		s1 += order.Uint32(a[i+4:]) + s0
	}
	return [2]uint32{s0, s1}
}
