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

# Enable the git daemon export flag
touch "$BARE_DIR/git-daemon-export-ok"

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

# --- 4. Serve bare repo via git daemon ---

echo "Serving git repo on git://0.0.0.0:9080 ..."
exec git daemon --verbose --reuseaddr \
    --base-path="$BARE_DIR" \
    --export-all \
    --listen=0.0.0.0 \
    --port=9080 \
    "$BARE_DIR"
