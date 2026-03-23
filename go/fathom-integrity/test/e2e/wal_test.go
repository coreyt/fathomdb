package e2e

import (
	"encoding/binary"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/coreyt/fathomdb/go/fathom-integrity/internal/walcheck"
	"github.com/coreyt/fathomdb/go/fathom-integrity/test/testutil"
	"github.com/stretchr/testify/require"
)

// buildWALChecksumBytes computes the SQLite WAL rolling checksum.
// Mirrors the algorithm in internal/walcheck so tests can build valid WAL files
// without depending on unexported package internals.
func buildWALChecksumBytes(a []byte, s [2]uint32) [2]uint32 {
	s0, s1 := s[0], s[1]
	for i := 0; i+8 <= len(a); i += 8 {
		s0 += binary.BigEndian.Uint32(a[i:]) + s1
		s1 += binary.BigEndian.Uint32(a[i+4:]) + s0
	}
	return [2]uint32{s0, s1}
}

// buildTestWAL constructs a WAL file with numFrames correctly checksummed frames.
// Uses non-zero salt values to ensure checksum correctness is meaningful.
func buildTestWAL(t *testing.T, pageSize uint32, numFrames int) []byte {
	t.Helper()
	const salt1 = uint32(0x12345678)
	const salt2 = uint32(0xABCDEF01)

	hdr := make([]byte, walcheck.WALHeaderSize)
	binary.BigEndian.PutUint32(hdr[0:4], 0x377f0682) // WAL magic BE
	binary.BigEndian.PutUint32(hdr[4:8], 3007000)
	binary.BigEndian.PutUint32(hdr[8:12], pageSize)
	binary.BigEndian.PutUint32(hdr[12:16], 0) // checkpoint seq
	binary.BigEndian.PutUint32(hdr[16:20], salt1)
	binary.BigEndian.PutUint32(hdr[20:24], salt2)
	ck := buildWALChecksumBytes(hdr[:24], [2]uint32{0, 0})
	binary.BigEndian.PutUint32(hdr[24:28], ck[0])
	binary.BigEndian.PutUint32(hdr[28:32], ck[1])

	data := hdr
	running := [2]uint32{salt1, salt2}
	for i := 0; i < numFrames; i++ {
		frame := make([]byte, walcheck.WALFrameHeaderSize+int(pageSize))
		binary.BigEndian.PutUint32(frame[0:4], uint32(i+1))
		binary.BigEndian.PutUint32(frame[8:12], salt1)
		binary.BigEndian.PutUint32(frame[12:16], salt2)
		running = buildWALChecksumBytes(frame[:8], running)
		running = buildWALChecksumBytes(frame[walcheck.WALFrameHeaderSize:], running)
		binary.BigEndian.PutUint32(frame[16:20], running[0])
		binary.BigEndian.PutUint32(frame[20:24], running[1])
		data = append(data, frame...)
	}
	return data
}

// checkReport is a subset of DiagnosticReport sufficient for WAL assertions.
type checkReport struct {
	Layer1 struct {
		WALPresent bool `json:"wal_present"`
		WAL        struct {
			Present         bool  `json:"present"`
			HeaderValid     bool  `json:"header_valid"`
			FrameCount      int   `json:"frame_count"`
			Truncated       bool  `json:"truncated"`
			TruncationOffset int64 `json:"truncation_offset"`
		} `json:"wal"`
		Findings []struct {
			Layer    int    `json:"layer"`
			Severity string `json:"severity"`
			Message  string `json:"message"`
		} `json:"findings"`
	} `json:"layer1"`
	Overall     string   `json:"overall"`
	Suggestions []string `json:"suggestions"`
}

func TestCheckCommand_DetectsWALBitFlip(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	sqliteBin := testutil.SQLiteBinary()
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	walPath := dbPath + "-wal"

	// Create a minimal valid SQLite database.
	out, err := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);").CombinedOutput()
	require.NoError(t, err, string(out))

	// Write a WAL file with 3 correctly checksummed frames.
	require.NoError(t, os.WriteFile(walPath, buildTestWAL(t, 4096, 3), 0o644))

	// Flip a byte in the page data of frame 2 (0-indexed: frame index 1).
	// This corrupts the rolling checksum chain from frame 2 onward.
	testutil.InjectWALBitFlip(t, walPath, 1, 0)

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity",
		"check",
		"--db", dbPath,
	)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()
	output, err := cmd.CombinedOutput()

	// check exits 0 even on findings — it's a diagnostic, not a gate.
	require.NoError(t, err, string(output))

	// Parse the JSON report from stdout.
	var report checkReport
	// The output contains a JSON line followed by "check completed".
	for _, line := range splitLines(string(output)) {
		if len(line) > 0 && line[0] == '{' {
			require.NoError(t, json.Unmarshal([]byte(line), &report), "parse check output: %s", line)
			break
		}
	}

	require.True(t, report.Layer1.WALPresent, "WAL should be detected as present")
	require.True(t, report.Layer1.WAL.HeaderValid, "WAL header should be valid")
	require.Equal(t, 1, report.Layer1.WAL.FrameCount, "only frame 1 should be valid")
	require.True(t, report.Layer1.WAL.Truncated, "WAL should be reported as truncated at the checksum failure point")
	require.Greater(t, report.Layer1.WAL.TruncationOffset, int64(0))

	// The WAL truncation should surface as a Layer 1 warning.
	foundWALFinding := false
	for _, f := range report.Layer1.Findings {
		if f.Layer == 1 && f.Severity == "warning" {
			foundWALFinding = true
		}
	}
	require.True(t, foundWALFinding, "expected Layer 1 warning finding for WAL truncation, findings: %+v", report.Layer1.Findings)
}

func TestCheckCommand_CleanWAL_NoTruncationFinding(t *testing.T) {
	repoRoot := filepath.Join("..", "..")
	sqliteBin := testutil.SQLiteBinary()
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "fathom.db")
	walPath := dbPath + "-wal"

	out, err := exec.Command(sqliteBin, dbPath, "CREATE TABLE test (id INTEGER);").CombinedOutput()
	require.NoError(t, err, string(out))

	// Write a clean WAL with valid checksums — no corruption.
	require.NoError(t, os.WriteFile(walPath, buildTestWAL(t, 4096, 2), 0o644))

	cmd := exec.Command("go", "run", "./cmd/fathom-integrity",
		"check",
		"--db", dbPath,
	)
	cmd.Dir = repoRoot
	cmd.Env = os.Environ()
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, string(output))

	var report checkReport
	for _, line := range splitLines(string(output)) {
		if len(line) > 0 && line[0] == '{' {
			require.NoError(t, json.Unmarshal([]byte(line), &report))
			break
		}
	}

	require.True(t, report.Layer1.WAL.HeaderValid)
	require.Equal(t, 2, report.Layer1.WAL.FrameCount)
	require.False(t, report.Layer1.WAL.Truncated)
}

func splitLines(s string) []string {
	var lines []string
	start := 0
	for i, c := range s {
		if c == '\n' {
			lines = append(lines, s[start:i])
			start = i + 1
		}
	}
	if start < len(s) {
		lines = append(lines, s[start:])
	}
	return lines
}
