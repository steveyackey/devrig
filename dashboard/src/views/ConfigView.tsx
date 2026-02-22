import { Component, createSignal, createEffect, onCleanup, Show } from 'solid-js';
import { fetchConfig, updateConfig } from '../api';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState } from '@codemirror/state';
import { json } from '@codemirror/lang-json';

// We use smol-toml for client-side TOML validation
import { parse as parseToml } from 'smol-toml';

// Inline dark theme since @codemirror/theme-one-dark is not installed
const darkTheme = EditorView.theme(
  {
    '&': {
      color: '#e4e4e7',
      backgroundColor: '#18181b',
    },
    '.cm-content': {
      caretColor: '#60a5fa',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: '#60a5fa',
    },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: '#3f3f46',
    },
    '.cm-panels': {
      backgroundColor: '#27272a',
      color: '#e4e4e7',
    },
    '.cm-panels.cm-panels-top': {
      borderBottom: '1px solid #3f3f46',
    },
    '.cm-panels.cm-panels-bottom': {
      borderTop: '1px solid #3f3f46',
    },
    '.cm-searchMatch': {
      backgroundColor: '#fbbf2440',
      outline: '1px solid #fbbf2480',
    },
    '.cm-searchMatch.cm-searchMatch-selected': {
      backgroundColor: '#60a5fa40',
    },
    '.cm-activeLine': {
      backgroundColor: '#27272a50',
    },
    '.cm-selectionMatch': {
      backgroundColor: '#3f3f4680',
    },
    '&.cm-focused .cm-matchingBracket, &.cm-focused .cm-nonmatchingBracket': {
      backgroundColor: '#52525b80',
    },
    '.cm-gutters': {
      backgroundColor: '#18181b',
      color: '#52525b',
      border: 'none',
    },
    '.cm-activeLineGutter': {
      backgroundColor: '#27272a50',
    },
    '.cm-foldPlaceholder': {
      backgroundColor: 'transparent',
      border: 'none',
      color: '#71717a',
    },
    '.cm-tooltip': {
      border: '1px solid #3f3f46',
      backgroundColor: '#27272a',
    },
    '.cm-tooltip .cm-tooltip-arrow:before': {
      borderTopColor: 'transparent',
      borderBottomColor: 'transparent',
    },
    '.cm-tooltip .cm-tooltip-arrow:after': {
      borderTopColor: '#27272a',
      borderBottomColor: '#27272a',
    },
    '.cm-tooltip-autocomplete': {
      '& > ul > li[aria-selected]': {
        backgroundColor: '#3f3f46',
        color: '#e4e4e7',
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

    // Client-side TOML validation
    const validErr = validateToml(content);
    if (validErr) {
      setValidationError(validErr);
      setSaveStatus('error');
      setSaveError(`Invalid TOML: ${validErr}`);
      return;
    }
    setValidationError(null);

    try {
      setSaveStatus('saving');
      setSaveError(null);
      const result = await updateConfig(content, hash());
      setHash(result.hash);
      setSaveStatus('saved');
      // Reset status after 3 seconds
      setTimeout(() => {
        if (saveStatus() === 'saved') setSaveStatus('idle');
      }, 3000);
    } catch (err: any) {
      if (err.message?.includes('modified externally')) {
        setSaveStatus('conflict');
        setSaveError('Config was modified externally. Reload to get the latest version.');
      } else {
        setSaveStatus('error');
        setSaveError(err.message || 'Failed to save');
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
      case 'saved': return 'text-green-400';
      case 'error': return 'text-red-400';
      case 'conflict': return 'text-yellow-400';
      case 'saving': return 'text-blue-400';
      default: return 'text-zinc-500';
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
      {/* Header */}
      <div class="px-6 py-4 border-b border-zinc-700/50 flex items-center justify-between">
        <div>
          <h2 class="text-lg font-semibold text-zinc-100">Configuration</h2>
          <p class="text-sm text-zinc-500 mt-0.5">Edit devrig.toml</p>
        </div>
        <div class="flex items-center gap-3">
          <Show when={statusText()}>
            <span class={`text-xs ${statusColor()}`}>{statusText()}</span>
          </Show>
          <Show when={saveStatus() === 'conflict'}>
            <button
              onClick={() => loadConfig()}
              class="bg-yellow-600 hover:bg-yellow-500 text-white text-sm px-3 py-1.5 rounded-md"
            >
              Reload
            </button>
          </Show>
          <button
            onClick={handleSave}
            disabled={saveStatus() === 'saving' || !!validationError()}
            class="bg-blue-600 hover:bg-blue-500 disabled:bg-zinc-700 disabled:text-zinc-500 text-white text-sm px-4 py-1.5 rounded-md transition-colors"
          >
            Save
          </button>
        </div>
      </div>

      {/* Editor */}
      <div class="flex-1 overflow-hidden">
        <Show when={error()}>
          <div class="m-6 bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-center">
            <p class="text-red-400 text-sm">{error()}</p>
            <button
              onClick={() => loadConfig()}
              class="mt-2 text-blue-400 hover:text-blue-300 text-sm"
            >
              Retry
            </button>
          </div>
        </Show>

        <Show when={loading() && !editorView}>
          <div class="py-12 text-center text-zinc-500 text-sm">
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
