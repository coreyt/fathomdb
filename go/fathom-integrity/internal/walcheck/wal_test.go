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
	buf := make([]byte, walHeaderSize)
	binary.BigEndian.PutUint32(buf[0:4], walMagicBE)
	binary.BigEndian.PutUint32(buf[4:8], 3007000)
	binary.BigEndian.PutUint32(buf[8:12], pageSize)
	binary.BigEndian.PutUint32(buf[12:16], checkpointSeq)
	// salt-1, salt-2, checksum-1, checksum-2 left as zero — sufficient for header parsing
	return buf
}

// buildWALFrame returns a complete frame (24-byte header + page data) of zeros.
func buildWALFrame(pageSize uint32) []byte {
	return make([]byte, walFrameHeaderSize+int(pageSize))
}

// writeWALFile writes data to a temp file and returns its path.
func writeWALFile(t *testing.T, data []byte) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "fathom.db-wal")
	require.NoError(t, os.WriteFile(path, data, 0o644))
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
	buf := make([]byte, walHeaderSize)
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
	data = append(data, buildWALFrame(pageSize)...)  // one complete frame
	partial := make([]byte, walFrameHeaderSize+100) // partial second frame
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
