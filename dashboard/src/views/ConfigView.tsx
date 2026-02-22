import { Component, createSignal, createEffect, onCleanup, Show } from 'solid-js';
import { fetchConfig, updateConfig } from '../api';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState } from '@codemirror/state';
import { json } from '@codemirror/lang-json';
import { parse as parseToml } from 'smol-toml';
import { Button } from '../components/ui';
import { showToast } from '../components/ui/toast';

// Dark theme matching the new design system
const darkTheme = EditorView.theme(
  {
    '&': {
      color: 'var(--color-text-primary)',
      backgroundColor: 'var(--color-surface-0)',
    },
    '.cm-content': {
      caretColor: 'var(--color-accent)',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--color-accent)',
    },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'var(--color-surface-3)',
    },
    '.cm-panels': {
      backgroundColor: 'var(--color-surface-2)',
      color: 'var(--color-text-primary)',
    },
    '.cm-panels.cm-panels-top': {
      borderBottom: '1px solid var(--color-border)',
    },
    '.cm-panels.cm-panels-bottom': {
      borderTop: '1px solid var(--color-border)',
    },
    '.cm-searchMatch': {
      backgroundColor: 'rgba(255, 214, 0, 0.15)',
      outline: '1px solid rgba(255, 214, 0, 0.3)',
    },
    '.cm-searchMatch.cm-searchMatch-selected': {
      backgroundColor: 'rgba(255, 214, 0, 0.25)',
    },
    '.cm-activeLine': {
      backgroundColor: 'rgba(30, 30, 26, 0.5)',
    },
    '.cm-selectionMatch': {
      backgroundColor: 'rgba(42, 42, 36, 0.5)',
    },
    '&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket': {
      backgroundColor: 'rgba(42, 42, 36, 0.8)',
    },
    '.cm-gutters': {
      backgroundColor: 'var(--color-surface-0)',
      color: 'var(--color-text-muted)',
      border: 'none',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'rgba(30, 30, 26, 0.5)',
    },
    '.cm-foldPlaceholder': {
      backgroundColor: 'transparent',
      border: 'none',
      color: 'var(--color-text-muted)',
    },
    '.cm-tooltip': {
      border: '1px solid var(--color-border)',
      backgroundColor: 'var(--color-surface-2)',
    },
    '.cm-tooltip .cm-tooltip-arrow:before': {
      borderTopColor: 'transparent',
      borderBottomColor: 'transparent',
    },
    '.cm-tooltip .cm-tooltip-arrow:after': {
      borderTopColor: 'var(--color-surface-2)',
      borderBottomColor: 'var(--color-surface-2)',
    },
    '.cm-tooltip-autocomplete': {
      '& > ul > li[aria-selected]': {
        backgroundColor: 'var(--color-surface-3)',
        color: 'var(--color-text-primary)',
      },
    },
  },
  { dark: true }
);

const ConfigView: Component = () => {
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [saveStatus, setSaveStatus] = createSignal<'idle' | 'saving' | 'saved' | 'error' | 'conflict'>('idle');
  const [saveError, setSaveError] = createSignal<string | null>(null);
  const [hash, setHash] = createSignal('');
  const [validationError, setValidationError] = createSignal<string | null>(null);

  let editorContainer: HTMLDivElement | undefined;
  let editorView: EditorView | undefined;

  const loadConfig = async () => {
    try {
      setError(null);
      setLoading(true);
      const data = await fetchConfig();
      setHash(data.hash);

      if (editorView) {
        editorView.dispatch({
          changes: {
            from: 0,
            to: editorView.state.doc.length,
            insert: data.content,
          },
        });
      }
    } catch (err: any) {
      setError(err.message || 'Failed to load config');
    } finally {
      setLoading(false);
    }
  };

  const validateToml = (content: string): string | null => {
    try {
      parseToml(content);
      return null;
    } catch (err: any) {
      return err.message || 'Invalid TOML';
    }
  };

  const handleSave = async () => {
    if (!editorView) return;

    const content = editorView.state.doc.toString();

    const validErr = validateToml(content);
    if (validErr) {
      setValidationError(validErr);
      setSaveStatus('error');
      setSaveError(`Invalid TOML: ${validErr}`);
      showToast(`Invalid TOML: ${validErr}`, 'error');
      return;
    }
    setValidationError(null);

    try {
      setSaveStatus('saving');
      setSaveError(null);
      const result = await updateConfig(content, hash());
      setHash(result.hash);
      setSaveStatus('saved');
      showToast('Configuration saved', 'success');
      setTimeout(() => {
        if (saveStatus() === 'saved') setSaveStatus('idle');
      }, 3000);
    } catch (err: any) {
      if (err.message?.includes('modified externally')) {
        setSaveStatus('conflict');
        setSaveError('Config was modified externally. Reload to get the latest version.');
        showToast('Config conflict — reload required', 'error');
      } else {
        setSaveStatus('error');
        setSaveError(err.message || 'Failed to save');
        showToast(err.message || 'Failed to save', 'error');
      }
    }
  };

  createEffect(() => {
    if (!editorContainer) return;

    const state = EditorState.create({
      doc: '',
      extensions: [
        basicSetup,
        json(),
        darkTheme,
        EditorView.theme({
          '&': { height: '100%' },
          '.cm-scroller': { overflow: 'auto' },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            setSaveStatus('idle');
            const content = update.state.doc.toString();
            const err = validateToml(content);
            setValidationError(err);
          }
        }),
      ],
    });

    editorView = new EditorView({
      state,
      parent: editorContainer,
    });

    loadConfig();

    onCleanup(() => {
      editorView?.destroy();
    });
  });

  const statusColor = () => {
    switch (saveStatus()) {
      case 'saved': return 'text-success';
      case 'error': return 'text-error';
      case 'conflict': return 'text-warning';
      case 'saving': return 'text-accent';
      default: return 'text-text-muted';
    }
  };

  const statusText = () => {
    switch (saveStatus()) {
      case 'saved': return 'Saved';
      case 'error': return saveError() || 'Error';
      case 'conflict': return saveError() || 'Conflict';
      case 'saving': return 'Saving...';
      default: return validationError() ? `TOML error: ${validationError()}` : '';
    }
  };

  return (
    <div class="flex flex-col h-full">
      <div class="px-8 py-6 border-b-2 border-border flex items-center justify-between">
        <div>
          <h2
            class="font-display text-4xl text-accent tracking-[0.1em] uppercase"
            style={{ "text-shadow": "2px 2px 0 rgba(0,0,0,0.5)" }}
          >
            Configuration
          </h2>
          <p class="font-label text-[10px] text-text-secondary uppercase tracking-[0.1em] mt-1">Edit devrig.toml</p>
        </div>
        <div class="flex items-center gap-3">
          <Show when={statusText()}>
            <span class={`text-xs ${statusColor()}`}>{statusText()}</span>
          </Show>
          <Show when={saveStatus() === 'conflict'}>
            <Button variant="destructive" size="sm" onClick={() => loadConfig()}>
              Reload
            </Button>
          </Show>
          <Button
            onClick={handleSave}
            disabled={saveStatus() === 'saving' || !!validationError()}
            size="sm"
          >
            Save
          </Button>
        </div>
      </div>

      <div class="flex-1 overflow-hidden">
        <Show when={error()}>
          <div class="m-6 bg-error/10 border border-error/20 rounded-lg p-4 text-center">
            <p class="text-error text-sm">{error()}</p>
            <button onClick={() => loadConfig()} class="mt-2 text-accent hover:text-accent-hover text-sm">Retry</button>
          </div>
        </Show>

        <Show when={loading() && !editorView}>
          <div class="py-12 text-center text-text-muted text-sm">
            Loading configuration...
          </div>
        </Show>

        <div
          ref={editorContainer!}
          class="h-full"
          style={{ "font-size": "14px" }}
        />
      </div>
    </div>
  );
};

export default ConfigView;
