#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$SCRIPT_DIR/gitrepo"
BARE_DIR="$SCRIPT_DIR/.bare-repo"
SEED_DIR="$SCRIPT_DIR/k8s/seed"

# --- 1. Ensure gitrepo/ is a git repo with seed content ---

if [ ! -d "$REPO_DIR" ]; then
    echo "Creating gitrepo/ with seed content..."
    mkdir -p "$REPO_DIR"
    cp -r "$SEED_DIR"/* "$REPO_DIR"/
    git -C "$REPO_DIR" init -b main
    git -C "$REPO_DIR" add -A
    git -C "$REPO_DIR" commit -m "Initial seed"
elif [ ! -d "$REPO_DIR/.git" ]; then
    echo "gitrepo/ exists but is not a git repo — initializing..."
    git -C "$REPO_DIR" init -b main
    git -C "$REPO_DIR" add -A
    git -C "$REPO_DIR" commit -m "Initial commit"
else
    echo "gitrepo/ already initialized — using as-is."
fi

# --- 2. Create/refresh bare clone ---

echo "Creating bare clone..."
rm -rf "$BARE_DIR"
git clone --bare "$REPO_DIR" "$BARE_DIR"

# Enable anonymous read/write for the smart HTTP protocol
git -C "$BARE_DIR" config http.receivepack true
git -C "$BARE_DIR" config core.sharedRepository true

# --- 3. Set bare repo as origin + install post-commit hook ---

git -C "$REPO_DIR" remote remove origin 2>/dev/null || true
git -C "$REPO_DIR" remote add origin "$BARE_DIR"

mkdir -p "$REPO_DIR/.git/hooks"
cat > "$REPO_DIR/.git/hooks/post-commit" << 'HOOK'
#!/usr/bin/env bash
set -euo pipefail
echo "Pushing to bare repo..."
git push origin main
echo "Bare repo updated — Flux will pick up changes within 30s."
HOOK
chmod +x "$REPO_DIR/.git/hooks/post-commit"

# --- 4. Serve bare repo via smart HTTP ---

GIT_HTTP_BACKEND="$(git --exec-path)/git-http-backend"

GIT_PORT="${PORT:-9080}"
echo "Serving git repo on http://0.0.0.0:$GIT_PORT ..."
exec python3 -c "
import os, subprocess, sys
from http.server import HTTPServer, BaseHTTPRequestHandler

BACKEND = '$GIT_HTTP_BACKEND'
REPO = '$BARE_DIR'

class GitHTTPHandler(BaseHTTPRequestHandler):
    def _run_backend(self, body=b''):
        env = {
            'GIT_PROJECT_ROOT': REPO,
            'GIT_HTTP_EXPORT_ALL': '1',
            'REQUEST_METHOD': self.command,
            'QUERY_STRING': self.path.split('?', 1)[1] if '?' in self.path else '',
            'PATH_INFO': self.path.split('?', 1)[0],
            'CONTENT_TYPE': self.headers.get('Content-Type', ''),
            'PATH': os.environ.get('PATH', ''),
        }
        proc = subprocess.run(
            [BACKEND], input=body, capture_output=True, env=env,
        )
        # Parse CGI response: headers then body separated by blank line
        raw = proc.stdout
        header_end = raw.find(b'\r\n\r\n')
        if header_end == -1:
            header_end = raw.find(b'\n\n')
            sep_len = 2
        else:
            sep_len = 4
        if header_end == -1:
            self.send_error(500)
            return
        header_bytes = raw[:header_end]
        body_bytes = raw[header_end + sep_len:]
        status = 200
        headers = {}
        for line in header_bytes.decode().split('\n'):
            line = line.strip()
            if line.lower().startswith('status:'):
                status = int(line.split(':', 1)[1].strip().split()[0])
            elif ':' in line:
                k, v = line.split(':', 1)
                headers[k.strip()] = v.strip()
        self.send_response(status)
        for k, v in headers.items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body_bytes)

    def do_GET(self):
        self._run_backend()

    def do_POST(self):
        length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(length) if length else b''
        self._run_backend(body)

    def log_message(self, fmt, *args):
        sys.stderr.write('%s\n' % (fmt % args))

HTTPServer(('0.0.0.0', $GIT_PORT), GitHTTPHandler).serve_forever()
"
