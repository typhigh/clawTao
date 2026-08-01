import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import { safeStringify, normalizeMd } from '../utils/format';
import { markdownComponents, sharedRemarkPlugins, sharedRehypePlugins } from '../utils/markdown';
import type { AssistantSegment, TurnSegment } from '../types/timeline';
import { WrenchIcon, PaperclipIcon, CheckIcon, CrossIcon } from './icons';
import { Thinking } from './Thinking';

// ── Helpers ──────────────────────────────────────────────────────────

/** Strip directory portion — handles both POSIX `/` and Windows `\`. */
export function basename(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts[parts.length - 1] || p;
}

/** Short, inline-friendly label that follows the tool name in the tool card
 *  header. Kept short on purpose — long paths/arguments would overflow the
 *  collapsed view and re-introduce the rendering cost this UI is meant
 *  to avoid.
 *
 *  - Read / Write / Edit → basename of `path`
 *  - Bash                → every command in a chained invocation joined
 *                          with ` && ` (e.g. `cd && npx`)
 *  - everything else     → empty (the header still shows the tool name)
 *
 *  Exported for unit testing.
 */
export function getToolSummary(name: string, input: unknown): string {
  if (!input || typeof input !== 'object') return '';
  const obj = input as Record<string, unknown>;
  switch (name) {
    case 'Read':
    case 'Write':
    case 'Edit': {
      const p = obj.path;
      return typeof p === 'string' && p.length > 0 ? basename(p) : '';
    }
    case 'Bash': {
      const cmd = obj.command;
      if (typeof cmd !== 'string' || cmd.length === 0) return '';
      // Split on `&&` only — `||`, `|`, `;` are uncommon in practice
      // and complicate the parser. Each segment's first whitespace-
      // delimited token is the command we display.
      //   `cd /tmp && npx tsc`               → `cd && npx`
      //   `npx vitest run 2>&1 | tail -8`    → `npx` (tail is not the
      //                                          first token — ignored)
      //   `cat /etc/passwd`                  → `cat`
      //   `./scripts/build.sh && ls dist`    → `build.sh && ls`
      const segments = cmd.split('&&').map((s) => s.trim()).filter(Boolean);
      const names = segments
        .map((seg) => {
          const m = seg.match(/^([^\s]+)/);
          return m ? basename(m[1]) : '';
        })
        .filter((n) => n.length > 0);
      if (names.length === 0) return '';
      return names.join(' && ');
    }
    default:
      return '';
  }
}

function getToolLink(name: string, input: unknown): string | null {
  if (name !== 'WebFetch' && name !== 'WebBrowser') return null;
  if (!input || typeof input !== 'object') return null;
  const url = (input as Record<string, unknown>).url;
  return typeof url === 'string' && url.length > 0 ? url : null;
}

function ToolCardBody({ toolInput, result }: { toolInput: unknown; result: string | null }) {
  const { t } = useTranslation();
  if (toolInput === null && result === null) return null;
  const both = toolInput !== null && result !== null;
  return (
    <div className="turn-segment-body tool-card">
      {toolInput !== null && (
        <div className="tool-input">
          <strong>{t('tool.input')}:</strong>
          <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{safeStringify(toolInput)}</pre>
        </div>
      )}
      {both && <div className="tool-result-divider" />}
      {result !== null && (
        <div className="tool-result">
          <strong>{t('tool.result')}:</strong>
          <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{result}</pre>
        </div>
      )}
    </div>
  );
}

// ── ToolCard ─────────────────────────────────────────────────────────

/** Decide whether a tool result string represents success. Returns
 *  `null` when the tool hasn't produced a result yet (caller should show
 *  a pending spinner instead of a status icon).
 *
 *  The Rust side doesn't carry an explicit success flag on `tool_result`
 *  events — it just serialises the result string. We have to infer:
 *   - "Tool error: …" / "Sandbox denied: …" / "Unknown tool: …" → fail
 *   - "(exit code: N)" trailing marker (bash only, present when the
 *     command produced no stdout/stderr) → N === 0 means success
 *   - any other content (including empty stdout/stderr for bash) → success
 *
 *  Exported for unit testing.
 */
export function toolResultSucceeded(result: string | null): boolean | null {
  if (result == null) return null;
  if (/^(Tool error:|Sandbox denied:|Unknown tool:)/.test(result)) return false;
  const m = result.match(/\(exit code: (-?\d+)\)\s*$/);
  if (m) return m[1] === '0';
  return true;
}

export function ToolCard({ toolName, toolInput, result, pending }: {
  toolName: string; toolInput: unknown; result: string | null; pending: boolean;
}) {
  const [open, setOpen] = useState(false);
  const linkUrl = getToolLink(toolName, toolInput);
  const summary = getToolSummary(toolName, toolInput);
  const succeeded = toolResultSucceeded(result);

  return (
    <div className={`turn-segment tool-pair ${pending ? 'pending' : 'done'}`}>
      <div className="tool-label-row">
        <button type="button" className="tool-label-btn" onClick={() => setOpen((o) => !o)}>
          <span className="tool-label-icon"><WrenchIcon /></span>
          <span className="tool-label-name">{toolName}</span>
          {summary && <span className="tool-label-summary">{summary}</span>}
          {!pending && succeeded !== null && (
            <span
              className={`tool-label-status ${succeeded ? 'ok' : 'fail'}`}
              title={succeeded ? 'OK' : 'Failed'}
              aria-label={succeeded ? 'Succeeded' : 'Failed'}
            >
              {succeeded ? <CheckIcon /> : <CrossIcon />}
            </span>
          )}
          <span className={`tool-label-arrow ${open ? 'open' : ''}`}>›</span>
          {pending && <span className="turn-segment-spinner" />}
        </button>
        {linkUrl && (
          <a className="tool-label-link" href={linkUrl} onClick={(e) => {
            e.preventDefault(); e.stopPropagation();
            window.electronAPI?.shell.openExternal(linkUrl);
          }} title={linkUrl}>
            <PaperclipIcon />
          </a>
        )}
      </div>
      {open && <ToolCardBody toolInput={toolInput} result={result} />}
    </div>
  );
}

// ── SegmentView + ToolPairView ───────────────────────────────────────

export function SegmentView({ segment }: { segment: AssistantSegment }) {
  const { t } = useTranslation();
  if (segment.kind === 'toolPair') {
    return <ToolCard toolName={segment.toolName} toolInput={segment.toolInput} result={segment.result} pending={segment.pending} />;
  }
  if (segment.kind === 'toolResult') {
    return (
      <div className="turn-segment tool-result">
        <div className="turn-segment-body tool-card">
          <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, margin: 0 }}>{segment.content}</pre>
        </div>
      </div>
    );
  }
  if (segment.kind === 'thinking') {
    return <Thinking content={segment.content} />;
  }
  if (segment.kind === 'text') {
    return (
      <div className="turn-segment turn-text">
        <ReactMarkdown remarkPlugins={sharedRemarkPlugins} rehypePlugins={sharedRehypePlugins} components={markdownComponents}>{normalizeMd(segment.content)}</ReactMarkdown>
      </div>
    );
  }
  return null;
}

export function ToolPairView({ segment }: { segment: Extract<TurnSegment, { kind: 'toolPair' }> }) {
  return <ToolCard toolName={segment.toolName} toolInput={segment.toolInput} result={segment.result} pending={segment.pending} />;
}
