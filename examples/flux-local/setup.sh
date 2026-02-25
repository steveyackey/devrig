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

# Update server info so dumb HTTP transport works
git -C "$BARE_DIR" update-server-info

# --- 3. Set bare repo as origin + install post-commit hook ---

git -C "$REPO_DIR" remote remove origin 2>/dev/null || true
git -C "$REPO_DIR" remote add origin "$BARE_DIR"

mkdir -p "$REPO_DIR/.git/hooks"
cat > "$REPO_DIR/.git/hooks/post-commit" << 'HOOK'
#!/usr/bin/env bash
set -euo pipefail
echo "Pushing to bare repo..."
git push origin main
BARE="$(git remote get-url origin)"
git -C "$BARE" update-server-info
echo "Bare repo updated — Flux will pick up changes within 30s."
HOOK
chmod +x "$REPO_DIR/.git/hooks/post-commit"

# --- 4. Serve bare repo over HTTP ---

echo "Serving git repo on http://0.0.0.0:9080 ..."
exec python3 -m http.server 9080 --directory "$BARE_DIR"
