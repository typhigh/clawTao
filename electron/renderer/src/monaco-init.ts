/**
 * Monaco Editor one-time initialisation.
 *
 * Must be imported exactly once, before any Monaco component renders
 * (imported in main.tsx before <App />).
 *
 * Does two things:
 *
 * 1. Tells @monaco-editor/react to use the locally-bundled monaco-editor
 *    package instead of downloading Monaco from a CDN.
 *
 * 2. Configures web workers via Vite's `?worker` import suffix so Monaco
 *    can run syntax highlighting / diff computation off the main thread.
 *    Without this you get "Could not create web worker(s)" and all
 *    language processing falls back to the UI thread.
 *
 * NOTE: "TextModel got disposed before DiffEditorWidget model got reset"
 * is a React StrictMode double-effect race. It is handled by the
 * keepCurrentOriginalModel / keepCurrentModifiedModel props on
 * <DiffEditor> — see DiffModal.tsx for details.
 */
import { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';

// ── 1. Use local Monaco bundle ────────────────────────────────────
loader.config({ monaco });

// ── 2. Web workers ────────────────────────────────────────────────
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
import cssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker';
import htmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker';
import tsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker';

(self as unknown as Window & typeof globalThis).MonacoEnvironment = {
  getWorker(_workerId: string, label: string): Worker {
    switch (label) {
      case 'json':
        return new jsonWorker();
      case 'css':
      case 'scss':
      case 'less':
        return new cssWorker();
      case 'html':
      case 'handlebars':
      case 'razor':
        return new htmlWorker();
      case 'typescript':
      case 'javascript':
        return new tsWorker();
      default:
        return new editorWorker();
    }
  },
};
