package bridge

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestRequestJSONShape(t *testing.T) {
	request := Request{
		DatabasePath: "/tmp/fathom.db",
		Command:      "rebuild_projections",
		Target:       "fts",
	}

	body, err := json.Marshal(request)

	require.NoError(t, err)
	require.Contains(t, string(body), `"database_path":"/tmp/fathom.db"`)
	require.Contains(t, string(body), `"command":"rebuild_projections"`)
	require.Contains(t, string(body), `"target":"fts"`)
}
