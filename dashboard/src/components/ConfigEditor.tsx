import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState } from '@codemirror/state';
import { json } from '@codemirror/lang-json';
import { parse as parseToml } from 'smol-toml';

// Dark theme for the editor
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
    '.cm-gutters': {
      backgroundColor: '#18181b',
      color: '#52525b',
      border: 'none',
    },
    '.cm-activeLine': {
      backgroundColor: '#27272a50',
    },
    '.cm-activeLineGutter': {
      backgroundColor: '#27272a50',
    },
    '.cm-tooltip': {
      border: '1px solid #3f3f46',
      backgroundColor: '#27272a',
    },
  },
  { dark: true }
);

export interface ConfigEditorProps {
  initialContent: string;
  onChange?: (content: string) => void;
  onValidationChange?: (error: string | null) => void;
}

const ConfigEditor: Component<ConfigEditorProps> = (props) => {
  let editorContainer: HTMLDivElement | undefined;
  let editorView: EditorView | undefined;

  const validateToml = (content: string): string | null => {
    try {
      parseToml(content);
      return null;
    } catch (err: any) {
      return err.message || 'Invalid TOML';
    }
  };

  createEffect(() => {
    if (!editorContainer) return;

    const state = EditorState.create({
      doc: props.initialContent || '',
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
            const content = update.state.doc.toString();
            const err = validateToml(content);
            props.onValidationChange?.(err);
            props.onChange?.(content);
          }
        }),
      ],
    });

    editorView = new EditorView({
      state,
      parent: editorContainer,
    });

    onCleanup(() => {
      editorView?.destroy();
    });
  });

  return (
    <div
      ref={editorContainer!}
      class="h-full"
      style={{ "font-size": "14px" }}
    />
  );
};

export default ConfigEditor;
