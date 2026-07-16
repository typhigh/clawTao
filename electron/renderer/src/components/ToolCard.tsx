import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import { safeStringify, normalizeMd } from '../utils/format';
import { markdownComponents, sharedRemarkPlugins, sharedRehypePlugins } from '../utils/markdown';
import type { AssistantSegment, TurnSegment } from '../types/timeline';
import { WrenchIcon, PaperclipIcon } from './icons';
import { Thinking } from './Thinking';

// ── Helpers ──────────────────────────────────────────────────────────

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

export function ToolCard({ toolName, toolInput, result, pending }: {
  toolName: string; toolInput: unknown; result: string | null; pending: boolean;
}) {
  const [open, setOpen] = useState(false);
  const linkUrl = getToolLink(toolName, toolInput);

  return (
    <div className={`turn-segment tool-pair ${pending ? 'pending' : 'done'}`}>
      <div className="tool-label-row">
        <button type="button" className="tool-label-btn" onClick={() => setOpen((o) => !o)}>
          <span className="tool-label-icon"><WrenchIcon /></span> {toolName}
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
