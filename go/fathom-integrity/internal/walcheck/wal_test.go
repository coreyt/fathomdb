package walcheck

import (
	"encoding/binary"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
)

// buildWALHeader writes a 32-byte valid WAL header into buf.
// pageSize must be a valid SQLite page size (512–65536, power of 2).
func buildWALHeader(pageSize uint32, checkpointSeq uint32) []byte {
	buf := make([]byte, WALHeaderSize)
	binary.BigEndian.PutUint32(buf[0:4], walMagicBE) //nolint:staticcheck
	binary.BigEndian.PutUint32(buf[4:8], 3007000)
	binary.BigEndian.PutUint32(buf[8:12], pageSize)
	binary.BigEndian.PutUint32(buf[12:16], checkpointSeq)
	// salt-1, salt-2, checksum-1, checksum-2 left as zero — sufficient for header parsing
	return buf
}

// buildWALFrame returns a complete frame (24-byte header + page data) of zeros.
func buildWALFrame(pageSize uint32) []byte {
	return make([]byte, WALFrameHeaderSize+int(pageSize))
}

// writeWALFile writes data to a temp file and returns its path.
func writeWALFile(t *testing.T, data []byte) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "fathom.db-wal")
	require.NoError(t, os.WriteFile(path, data, 0o644)) //nolint:gosec // G306: test data file with conservative 0o644 permissions
	return path
}

func TestInspectWAL_NotPresent(t *testing.T) {
	report, err := InspectWAL("/nonexistent/path/fathom.db-wal")

	require.NoError(t, err)
	require.False(t, report.Present)
	require.False(t, report.HeaderValid)
	require.Equal(t, 0, report.FrameCount)
}

func TestInspectWAL_EmptyFile(t *testing.T) {
	path := writeWALFile(t, []byte{})

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.Present)
	require.False(t, report.HeaderValid)
}

func TestInspectWAL_InvalidMagic(t *testing.T) {
	buf := make([]byte, WALHeaderSize)
	binary.BigEndian.PutUint32(buf[0:4], 0xDEADBEEF)
	binary.BigEndian.PutUint32(buf[8:12], 4096)
	path := writeWALFile(t, buf)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.Present)
	require.False(t, report.HeaderValid)
}

func TestInspectWAL_ValidHeaderNoFrames(t *testing.T) {
	path := writeWALFile(t, buildWALHeader(4096, 7))

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.Present)
	require.True(t, report.HeaderValid)
	require.Equal(t, 4096, report.PageSize)
	require.Equal(t, uint32(7), report.CheckpointSeq)
	require.Equal(t, 0, report.FrameCount)
	require.False(t, report.Truncated)
	require.False(t, report.CheckpointNeeded)
}

func TestInspectWAL_OneCompleteFrame(t *testing.T) {
	const pageSize = 4096
	var data []byte
	data = append(data, buildWALHeader(pageSize, 0)...)
	data = append(data, buildWALFrame(pageSize)...)
	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Equal(t, 1, report.FrameCount)
	require.False(t, report.Truncated)
	require.Equal(t, int64(0), report.TruncationOffset)
}

func TestInspectWAL_TruncatedMidFrame(t *testing.T) {
	const pageSize = 4096
	// Header + 24-byte frame header + 100 bytes of page data (short by pageSize-100)
	var data []byte
	data = append(data, buildWALHeader(pageSize, 0)...)
	data = append(data, buildWALFrame(pageSize)...) // one complete frame
	partial := make([]byte, WALFrameHeaderSize+100) // partial second frame
	data = append(data, partial...)
	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Equal(t, 1, report.FrameCount) // one complete frame before truncation
	require.True(t, report.Truncated)
	require.Greater(t, report.TruncationOffset, int64(0))
}

func TestInspectWAL_MultipleFrames(t *testing.T) {
	const pageSize = 4096
	var data []byte
	data = append(data, buildWALHeader(pageSize, 0)...)
	for i := 0; i < 3; i++ {
		data = append(data, buildWALFrame(pageSize)...)
	}
	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Equal(t, 3, report.FrameCount)
	require.False(t, report.Truncated)
}

func TestInspectWAL_CheckpointNeeded(t *testing.T) {
	const pageSize = 4096
	var data []byte
	data = append(data, buildWALHeader(pageSize, 0)...)
	for i := 0; i < DefaultCheckpointThreshold+1; i++ {
		data = append(data, buildWALFrame(pageSize)...)
	}
	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Equal(t, DefaultCheckpointThreshold+1, report.FrameCount)
	require.True(t, report.CheckpointNeeded)
}

func TestInspectWAL_CheckpointNotNeededAtThreshold(t *testing.T) {
	const pageSize = 4096
	var data []byte
	data = append(data, buildWALHeader(pageSize, 0)...)
	for i := 0; i < DefaultCheckpointThreshold; i++ {
		data = append(data, buildWALFrame(pageSize)...)
	}
	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.Equal(t, DefaultCheckpointThreshold, report.FrameCount)
	require.False(t, report.CheckpointNeeded)
}

// buildChecksummedWAL constructs a WAL file with correctly computed frame checksums.
// numFrames frames are written with distinct page numbers. Salt values are non-zero
// to make the test meaningful (all-zero salt would trivially pass checksums of zero data).
func buildChecksummedWAL(t *testing.T, pageSize uint32, numFrames int) []byte {
	t.Helper()
	const salt1 = uint32(0xDEADBEEF)
	const salt2 = uint32(0xCAFEBABE)

	hdr := buildWALHeader(pageSize, 0)
	binary.BigEndian.PutUint32(hdr[16:20], salt1)
	binary.BigEndian.PutUint32(hdr[20:24], salt2)

	// Compute WAL header checksum (covers first 24 bytes, initial s0=s1=0).
	ck := walChecksumBytes(hdr[:24], [2]uint32{0, 0}, binary.BigEndian)
	binary.BigEndian.PutUint32(hdr[24:28], ck[0])
	binary.BigEndian.PutUint32(hdr[28:32], ck[1])

	data := hdr
	running := [2]uint32{salt1, salt2}

	for i := 0; i < numFrames; i++ {
		frame := make([]byte, WALFrameHeaderSize+int(pageSize))
		binary.BigEndian.PutUint32(frame[0:4], uint32(i+1)) //nolint:gosec // G115: loop index bounded by small test frame count, fits uint32
		binary.BigEndian.PutUint32(frame[4:8], 0)           // db size (non-commit frame)
		binary.BigEndian.PutUint32(frame[8:12], salt1)
		binary.BigEndian.PutUint32(frame[12:16], salt2)
		// Checksum covers the 8-byte frame header + page data.
		running = walChecksumBytes(frame[:8], running, binary.BigEndian)
		running = walChecksumBytes(frame[WALFrameHeaderSize:], running, binary.BigEndian)
		binary.BigEndian.PutUint32(frame[16:20], running[0])
		binary.BigEndian.PutUint32(frame[20:24], running[1])
		data = append(data, frame...)
	}
	return data
}

func TestInspectWAL_ValidChecksums_CountsAllFrames(t *testing.T) {
	const pageSize = 4096
	path := writeWALFile(t, buildChecksummedWAL(t, pageSize, 3))

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Equal(t, 3, report.FrameCount)
	require.False(t, report.Truncated)
}

func TestInspectWAL_ChecksumMismatch_StopsAtBadFrame(t *testing.T) {
	const pageSize = 4096
	// Build a 3-frame WAL with valid checksums, then corrupt the second frame's page data.
	data := buildChecksummedWAL(t, pageSize, 3)

	// Flip a byte in the page data of frame 2 (index 1).
	// Frame 2 starts at: WALHeaderSize + 1*(WALFrameHeaderSize+pageSize)
	frame2Start := WALHeaderSize + WALFrameHeaderSize + pageSize
	data[frame2Start+WALFrameHeaderSize+100] ^= 0xFF // corrupt page data

	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.True(t, report.HeaderValid)
	require.Equal(t, 1, report.FrameCount) // only frame 1 is valid
	require.True(t, report.Truncated)
	require.Greater(t, report.TruncationOffset, int64(0))
}

func TestInspectWAL_SaltMismatch_StopsAtBadFrame(t *testing.T) {
	const pageSize = 4096
	data := buildChecksummedWAL(t, pageSize, 2)

	// Corrupt the salt-1 field of frame 2 (bytes 8-11 of frame 2 header).
	frame2Start := WALHeaderSize + WALFrameHeaderSize + pageSize
	binary.BigEndian.PutUint32(data[frame2Start+8:frame2Start+12], 0xBADBADBD)

	path := writeWALFile(t, data)

	report, err := InspectWAL(path)

	require.NoError(t, err)
	require.Equal(t, 1, report.FrameCount) // frame 1 valid, frame 2 has bad salt
	require.True(t, report.Truncated)
}
