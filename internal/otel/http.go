package otel

import (
	"fmt"
	"io"
	"net/http"
)

func readBody(req *http.Request) ([]byte, error) {
	if req.Body == nil {
		return nil, fmt.Errorf("empty body")
	}
	defer req.Body.Close()
	return io.ReadAll(req.Body)
}
