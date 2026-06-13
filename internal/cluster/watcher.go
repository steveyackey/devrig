package cluster

import (
	"context"
	"io/fs"
	"path/filepath"
	"strings"
	"time"

	"github.com/fsnotify/fsnotify"
)

// ignoredDirs are directory names that are never watched.
var ignoredDirs = map[string]bool{
	".git": true, "target": true, "node_modules": true,
	".devrig": true, ".claude": true, "__pycache__": true,
}

// ignoredExts are file extensions that are never treated as changes.
var ignoredExts = map[string]bool{
	".swp": true, ".swo": true, ".tmp": true,
	".pyc": true, ".pyo": true,
}

// RebuildFunc is called with the context when a file change is detected.
type RebuildFunc func(ctx context.Context) error

// WatchAndRebuild watches contextDir for changes and calls rebuild when
// files change. It debounces by 500ms and cancels in-progress rebuilds on
// new changes. It blocks until ctx is cancelled.
func WatchAndRebuild(ctx context.Context, contextDir string, rebuild RebuildFunc) error {
	w, err := fsnotify.NewWatcher()
	if err != nil {
		return err
	}
	defer w.Close()

	if err := addRecursive(w, contextDir); err != nil {
		return err
	}

	var (
		rebuildCancel context.CancelFunc
		timer         *time.Timer
	)

	scheduleRebuild := func() {
		if timer != nil {
			timer.Stop()
		}
		timer = time.AfterFunc(500*time.Millisecond, func() {
			if rebuildCancel != nil {
				rebuildCancel()
			}
			rebuildCtx, cancel := context.WithCancel(ctx)
			rebuildCancel = cancel
			go func() {
				_ = rebuild(rebuildCtx)
			}()
		})
	}

	for {
		select {
		case <-ctx.Done():
			if rebuildCancel != nil {
				rebuildCancel()
			}
			return ctx.Err()
		case event, ok := <-w.Events:
			if !ok {
				return nil
			}
			if shouldIgnore(event.Name) {
				continue
			}
			scheduleRebuild()
		case <-w.Errors:
			// Non-fatal; continue watching.
		}
	}
}

// addRecursive adds the directory and all non-ignored subdirectories to the watcher.
func addRecursive(w *fsnotify.Watcher, root string) error {
	return filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil // skip unreadable dirs
		}
		if d.IsDir() {
			if ignoredDirs[d.Name()] {
				return filepath.SkipDir
			}
			return w.Add(path)
		}
		return nil
	})
}

func shouldIgnore(name string) bool {
	base := filepath.Base(name)
	ext := strings.ToLower(filepath.Ext(base))
	if ignoredExts[ext] {
		return true
	}
	for dir := range ignoredDirs {
		if strings.Contains(filepath.ToSlash(name), "/"+dir+"/") ||
			strings.HasSuffix(filepath.ToSlash(name), "/"+dir) {
			return true
		}
	}
	return false
}
