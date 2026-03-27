package bridge

import "testing"

func FuzzDecodeResponse(f *testing.F) {
	f.Add([]byte(`{"protocol_version":1,"ok":true,"message":"ok","payload":{}}`))
	f.Add([]byte(`{"protocol_version":99,"ok":true,"message":"ok","payload":{}}`))
	f.Add([]byte(`not-json`))

	f.Fuzz(func(t *testing.T, body []byte) {
		_, _ = decodeResponse(body)
	})
}
