import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useChatStore, Message, StreamEvent } from './stores/chat';
import { useSettingsStore } from './stores/settings';
import { SettingsDialog } from './components/SettingsDialog';
import { LanguageSwitcher } from './components/LanguageSwitcher';
import type { TFunction } from 'i18next';

// ── Timeline model ────────────────────────────────────────────────────
// Two sources feed the timeline:
//   1. Persisted messages from the Rust session store → historical turns
//   2. Live StreamEvent[] from the unified chat.stream channel → the live turn
//
// The live turn is rendered as a flat chronological list of segments
// (text and toolPair in the order they actually occurred). Historical
// turns use a collapsible fold for concise re-reading.

type AssistantSegment =
  | { kind: 'text'; id: string; content: string; timestamp: number }
  | { kind: 'tool'; id: string; toolName: string; toolInput: unknown; timestamp: number }
  | { kind: 'toolResult'; id: string; content: string; toolCallId?: string; timestamp: number }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean }
  | { kind: 'thinking'; id: string; content: string; timestamp: number };

type TurnSegment =
  | { kind: 'text'; content: string }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean }
  | { kind: 'thinking'; content: string };

type TimelineGroup =
  | { kind: 'user'; id: string; content: string; timestamp: number }
  | { kind: 'agentTurn'; id: string; segments: AssistantSegment[]; conclusion: string | null; isStreaming: boolean }
  | { kind: 'liveTurn'; id: string; segments: TurnSegment[]; isStreaming: boolean };

// ── Historical turns: messages → TimelineGroup[] ─────────────────────

function buildHistoricalTurns(messages: Message[]): TimelineGroup[] {
  const groups: TimelineGroup[] = [];
  let currentTurn: AssistantSegment[] | null = null;
  let currentConclusion: string | null = null;
  let turnIdCounter = 0;

  const flushTurn = () => {
    if ((currentTurn && currentTurn.length > 0) || currentConclusion !== null) {
      groups.push({
        kind: 'agentTurn',
        id: `turn-${turnIdCounter++}`,
        segments: currentTurn ?? [],
        conclusion: currentConclusion,
        isStreaming: false,
      });
    }
    currentTurn = null;
    currentConclusion = null;
  };

  for (const msg of messages) {
    if (msg.role === 'user') {
      flushTurn();
      groups.push({ kind: 'user', id: msg.id, content: msg.content, timestamp: msg.timestamp });
    } else if (msg.role === 'assistant') {
      if (msg.tool_calls && msg.tool_calls.length > 0) {
        if (!currentTurn) currentTurn = [];
        if (msg.thinking) {
          currentTurn.push({
            kind: 'thinking',
            id: `${msg.id}-thinking`,
            content: msg.thinking,
            timestamp: msg.timestamp,
          });
        }
        if (msg.content) {
          currentTurn.push({
            kind: 'text',
            id: `${msg.id}-text`,
            content: msg.content,
            timestamp: msg.timestamp,
          });
        }
        for (const tc of msg.tool_calls) {
          let parsedArgs: unknown = tc.function.arguments;
          try { parsedArgs = JSON.parse(tc.function.arguments); } catch { /* keep as string */ }
          currentTurn.push({
            kind: 'tool',
            id: tc.id,
            toolName: tc.function.name,
            toolInput: parsedArgs,
            timestamp: msg.timestamp,
          });
        }
      } else if (msg.content || msg.thinking) {
        if (msg.thinking) {
          if (!currentTurn) currentTurn = [];
          currentTurn.push({
            kind: 'thinking',
            id: `${msg.id}-thinking`,
            content: msg.thinking,
            timestamp: msg.timestamp,
          });
        }
        currentConclusion = msg.content;
      }
    } else if (msg.role === 'tool') {
      if (!currentTurn) currentTurn = [];
      currentTurn.push({
        kind: 'toolResult',
        id: msg.id,
        content: msg.content,
        toolCallId: msg.tool_call_id,
        timestamp: msg.timestamp,
      });
    }
  }
  flushTurn();
  return groups;
}

// ── Live turn: StreamEvent[] → flat chronological TurnSegment[] ──────
//
// Text deltas are accumulated so consecutive text doesn't create a
// segment per token. When a tool_call arrives, any buffered text is
// flushed as a text segment BEFORE the tool card — preserving the real
// execution order. Text that arrives between tool_call and tool_result
// is likewise flushed between them.

function buildLiveSegments(events: StreamEvent[]): { segments: TurnSegment[]; isStreaming: boolean } {
  const hasDone = events.some((e) => e.kind === 'done');
  const segments: TurnSegment[] = [];
  let textBuf = '';
  let thinkingBuf = '';
  let pendingTool: { id: string; name: string; input: unknown } | null = null;

  const flushThinking = () => {
    if (thinkingBuf) {
      segments.push({ kind: 'thinking', content: thinkingBuf });
      thinkingBuf = '';
    }
  };

  const flushText = () => {
    if (textBuf) {
      segments.push({ kind: 'text', content: textBuf });
      textBuf = '';
    }
  };

  const flushPending = () => {
    if (pendingTool) {
      segments.push({
        kind: 'toolPair',
        id: pendingTool.id,
        toolName: pendingTool.name,
        toolInput: pendingTool.input,
        result: null,
        pending: true,
      });
      pendingTool = null;
    }
  };

  for (const ev of events) {
    switch (ev.kind) {
      case 'started':
      case 'done':
        break;
      case 'thinking':
        thinkingBuf += ev.delta!;
        break;
      case 'text':
        flushThinking();
        textBuf += ev.delta!;
        break;
      case 'tool_call':
        flushThinking();
        flushText();
        flushPending();
        pendingTool = { id: ev.toolCallId!, name: ev.toolName!, input: ev.input };
        break;
      case 'tool_result':
        // Flush any text that arrived while the tool was running
        flushText();
        if (pendingTool && pendingTool.id === ev.toolCallId) {
          segments.push({
            kind: 'toolPair',
            id: pendingTool.id,
            toolName: pendingTool.name,
            toolInput: pendingTool.input,
            result: ev.output!,
            pending: false,
          });
          pendingTool = null;
        } else {
          // Orphaned result — no preceding tool_call in this turn
          segments.push({
            kind: 'toolPair',
            id: ev.toolCallId!,
            toolName: ev.toolName!,
            toolInput: null,
            result: ev.output!,
            pending: false,
          });
        }
        break;
    }
  }

  flushThinking();
  flushText();
  flushPending();

  return { segments, isStreaming: !hasDone };
}

// ── Segment pairing (for historical turns) ───────────────────────────

function pairToolWithResults(segments: AssistantSegment[]): AssistantSegment[] {
  const out: AssistantSegment[] = [];
  // Use a Map keyed by toolCallId — a single assistant message can contain
  // multiple parallel tool_calls, so a single lastTool variable isn't enough.
  const pending = new Map<string, Extract<AssistantSegment, { kind: 'tool' }>>();

  for (const s of segments) {
    if (s.kind === 'tool') {
      pending.set(s.id, s);
    } else if (s.kind === 'toolResult') {
      const matched = s.toolCallId ? pending.get(s.toolCallId) : undefined;
      if (matched) {
        pending.delete(s.toolCallId!);
        out.push({ kind: 'toolPair', id: matched.id, toolName: matched.toolName, toolInput: matched.toolInput, result: s.content, pending: false });
      } else {
        out.push(s);
      }
    } else {
      out.push(s);
    }
  }

  // Any tools that never received a result
  for (const tool of pending.values()) {
    out.push({ kind: 'toolPair', id: tool.id, toolName: tool.toolName, toolInput: tool.toolInput, result: null, pending: true });
  }

  return out;
}

function countTurnSegments(segments: AssistantSegment[]): { toolCount: number; processCount: number } {
  let toolCount = 0;
  let processCount = 0;
  for (const s of segments) {
    if (s.kind === 'tool' || s.kind === 'toolPair') toolCount++;
    else if (s.kind === 'text') processCount++;
  }
  return { toolCount, processCount };
}

// ── Helpers ───────────────────────────────────────────────────────────

function formatRelative(t: TFunction, timestamp: number): string {
  const diffMs = Date.now() - timestamp;
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return t('time.justNow');
  const min = Math.floor(sec / 60);
  if (min < 60) return t('time.minutesAgo', { n: min });
  const hr = Math.floor(min / 60);
  if (hr < 24) return t('time.hoursAgo', { n: hr });
  const day = Math.floor(hr / 24);
  if (day < 30) return t('time.daysAgo', { n: day });
  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function safeStringify(v: unknown): string {
  if (typeof v === 'string') return v;
  try { return JSON.stringify(v); } catch { return String(v); }
}

// ── Markdown helpers ────────────────────────────────────────────────────

/**
 * Collapse sequences of 3+ newlines into a single blank line (2 newlines).
 * This keeps intentional paragraph breaks while removing excessive spacing.
 */
function normalizeMd(text: string): string {
  return text.replace(/\n{3,}/g, '\n\n');
}

// ── App ───────────────────────────────────────────────────────────────

function App() {
  const { t } = useTranslation();
  const {
    sessions, activeSessionId,
    loadSessions, createSession, selectSession, deleteSession,
    currentTurn, isStreaming,
    error, clearError,
    handleStreamEvent,
  } = useChatStore();

  const { config, loaded, load: loadConfig } = useSettingsStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  // const initRef = useRef(false);

  useEffect(() => {
    loadSessions();
    loadConfig();
    window.electronAPI.onStreamEvent(handleStreamEvent);
    return () => {
      window.electronAPI.onStreamEvent(() => {});
    };
  }, []);

  useEffect(() => {
    if (loaded && config && !config.api_key) {
      setSettingsOpen(true);
    }
  }, [loaded, config]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim() || isStreaming) return;
    const text = inputValue;
    setInputValue('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      // Re-apply the 2-line minimum so the box doesn't snap back to 1 line.
      textareaRef.current.style.height = `${2 * 24}px`;
    }
    await useChatStore.getState().sendMessage(text);
  };

  // Auto-grow the textarea as the user types, up to a max of ~6 lines.
  // Starts at 2 lines tall so the user has visible room to compose.
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = 'auto';
    const minHeight = 2 * 24; // 2 lines baseline
    const maxHeight = 6 * 24; // ~6 lines cap before scroll
    const target = Math.max(minHeight, Math.min(ta.scrollHeight, maxHeight));
    ta.style.height = `${target}px`;
  }, [inputValue]);

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      handleSubmit(e as unknown as React.FormEvent);
    }
  };

  const activeSession = sessions.find((s) => s.id === activeSessionId);

  // Historical turns from persisted messages, plus a live turn from stream events
  const historicalGroups = activeSession ? buildHistoricalTurns(activeSession.messages) : [];
  const live = currentTurn.length > 0 ? buildLiveSegments(currentTurn) : null;
  const timeline: TimelineGroup[] = [
    ...historicalGroups,
    ...(live ? [{ kind: 'liveTurn' as const, id: 'live-turn', segments: live.segments, isStreaming: live.isStreaming }] : []),
  ];

  return (
    <div className="app">
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <div className="main-content">
        <aside className="sidebar">
          <div className="sidebar-section config-section">
            <div className="sidebar-section-header">
              <h2>{t('sidebar.configuration')}</h2>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <LanguageSwitcher />
                <button className="btn-icon" onClick={() => setSettingsOpen(true)} title={t('sidebar.configuration')}>
                  ⚙️
                </button>
              </div>
            </div>
          </div>

          <div className="sidebar-spacer" />

          <div className="sidebar-section sessions-section">
            <div className="sidebar-section-header">
              <h2>{t('sidebar.sessions')}</h2>
              <button onClick={createSession} title={t('sidebar.newSession')}>+</button>
            </div>
            <div className="session-items">
              {sessions.length === 0 ? (
                <div className="session-empty">{t('sidebar.noSessions')}</div>
              ) : (
                sessions.map((session) => {
                  const isActive = session.id === activeSessionId;
                  const isRunning = isActive && isStreaming;
                  return (
                    <div
                      key={session.id}
                      className={`session-item ${isActive ? 'active' : ''}`}
                      onClick={() => selectSession(session.id)}
                    >
                      <span className={`session-item-status ${isRunning ? 'running' : 'done'}`}>
                        {isRunning ? <SpinnerIcon /> : <CheckIcon />}
                      </span>
                      <div className="session-item-title">{(session as any).title || t('sidebar.emptySession')}</div>
                      <div className="session-item-time">{formatRelative(t, session.updated_at)}</div>
                      <button
                        className="session-delete-btn"
                        onClick={(e) => { e.stopPropagation(); deleteSession(session.id); }}
                        title={t('sidebar.deleteSession')}
                      >×</button>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </aside>

        <main className="chat-area">
          {error && <div className="error" onClick={clearError}>{error}</div>}

          {activeSession ? (
            <>
              <div className="messages">
                {timeline.map((group) => {
                  if (group.kind === 'user') {
                    return <UserMessageView key={group.id} content={group.content} />;
                  }
                  if (group.kind === 'liveTurn') {
                    return (
                      <LiveTurnView
                        key="live-turn"
                        segments={group.segments}
                        isStreaming={group.isStreaming}
                      />
                    );
                  }
                  const turnKey = `${group.id}-done`;
                  return (
                    <AgentTurnView
                      key={turnKey}
                      segments={group.segments}
                      conclusion={group.conclusion}
                    />
                  );
                })}
              </div>

              <form className="input-area" onSubmit={handleSubmit}>
                <textarea
                  ref={textareaRef}
                  rows={2}
                  placeholder={t('chat.placeholder')}
                  value={inputValue}
                  onChange={(e) => setInputValue(e.target.value)}
                  onKeyDown={handleInputKeyDown}
                  disabled={isStreaming}
                />
                <button type="submit" disabled={!inputValue.trim() || isStreaming}>{t('chat.send')}</button>
              </form>
            </>
          ) : (
            <div className="empty-state">
              <p>{t('chat.noSession')}</p>
              <button onClick={createSession}>{t('chat.createSession')}</button>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

// ── Sub-components ────────────────────────────────────────────────────

function UserMessageView({ content }: { content: string }) {
  return (
    <div className="message user">
      <div className="message-content">{content}</div>
    </div>
  );
}

/** Live turn: flat chronological segments, no fold. */
function LiveTurnView({ segments, isStreaming }: { segments: TurnSegment[]; isStreaming: boolean }) {
  return (
    <div className={`agent-turn live ${isStreaming ? 'streaming' : ''}`}>
      {segments.map((seg, i) => {
        if (seg.kind === 'text') {
          return <div key={i} className="turn-text"><ReactMarkdown remarkPlugins={[remarkGfm]}>{normalizeMd(seg.content)}</ReactMarkdown></div>;
        }
        if (seg.kind === 'thinking') {
          return <Thinking key={i} content={seg.content} />;
        }
        return <ToolPairView key={seg.id} segment={seg} />;
      })}
    </div>
  );
}

/** Thinking text rendered inline in blue (#007aff). Hides when disabled. */
function Thinking({ content }: { content: string }) {
  const { config } = useSettingsStore();
  if (!config?.thinking_enabled) return null;
  return (
    <div className="turn-thinking">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{normalizeMd(content)}</ReactMarkdown>
    </div>
  );
}

/** Historical turn: tools folded, conclusion always visible. */
function AgentTurnView({ segments, conclusion }: { segments: AssistantSegment[]; conclusion: string | null }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const processSegments = pairToolWithResults(segments);
  const { toolCount, processCount } = countTurnSegments(processSegments);
  const hasProcessContent = processSegments.length > 0;

  return (
    <div className="agent-turn">
      {hasProcessContent && (
        <button type="button" className="agent-turn-header" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
          <span className={`agent-turn-caret ${open ? 'open' : ''}`}>›</span>
          <span className="agent-turn-title">
            {t('turn.summary', { tools: toolCount, messages: processCount })}
          </span>
        </button>
      )}
      {open && hasProcessContent && (
        <div className="agent-turn-body">
          {processSegments.map((seg) => (
            <SegmentView key={seg.id} segment={seg} />
          ))}
        </div>
      )}
      {conclusion && <div className="agent-turn-conclusion"><ReactMarkdown remarkPlugins={[remarkGfm]}>{normalizeMd(conclusion)}</ReactMarkdown></div>}
    </div>
  );
}

function ToolCard({ toolName, toolInput, result, pending }: { toolName: string; toolInput: unknown; result: string | null; pending: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const linkUrl = (toolName === 'WebFetch' || toolName === 'WebBrowser') && toolInput && typeof toolInput === 'object'
    ? (toolInput as any).url
    : null;
  const hasLink = typeof linkUrl === 'string' && linkUrl.length > 0;
  return (
    <div className={`turn-segment tool-pair ${pending ? 'pending' : 'done'}`}>
      <div className="tool-label-row">
        <button type="button" className="tool-label-btn" onClick={() => setOpen((o) => !o)}>
          <span className="tool-label-icon"><WrenchIcon /></span> {toolName}
          <span className={`tool-label-arrow ${open ? 'open' : ''}`}>›</span>
          {pending && <span className="turn-segment-spinner" />}
        </button>
        {hasLink && (
          <a
            className="tool-label-link"
            href={linkUrl}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              window.electronAPI?.shell.openExternal(linkUrl);
            }}
            title={linkUrl}
          >
            <PaperclipIcon />
          </a>
        )}
      </div>
      {open && (
        <div className="turn-segment-body tool-card">
          {toolInput !== null && (
            <div className="tool-input">
              <strong>{t('tool.input')}:</strong>
              <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{safeStringify(toolInput)}</pre>
            </div>
          )}
          {result !== null && (
            <>
              {toolInput !== null && <div className="tool-result-divider" />}
              <div className="tool-result">
                <strong>{t('tool.result')}:</strong>
                <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{result}</pre>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function SegmentView({ segment }: { segment: AssistantSegment }) {
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
    return <div className="turn-segment turn-text"><ReactMarkdown remarkPlugins={[remarkGfm]}>{normalizeMd(segment.content)}</ReactMarkdown></div>;
  }
  return null;
}

/** Tool card used in the live turn (receives TurnSegment, already paired). */
function ToolPairView({ segment }: { segment: Extract<TurnSegment, { kind: 'toolPair' }> }) {
  return <ToolCard toolName={segment.toolName} toolInput={segment.toolInput} result={segment.result} pending={segment.pending} />;
}

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M3 8.5L6.5 12L13 4.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function SpinnerIcon() {
  return (
    <svg className="session-spinner" width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeDasharray="28" strokeDashoffset="20" />
    </svg>
  );
}

/** Wrench / spanner — tool icon used in tool-call rows. (Lucide) */
function WrenchIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.106-3.105c.32-.322.863-.22.983.218a6 6 0 0 1-8.259 7.057l-7.91 7.91a1 1 0 0 1-2.999-3l7.91-7.91a6 6 0 0 1 7.057-8.259c.438.12.54.662.219.984z" />
    </svg>
  );
}

/** Paperclip icon — shown next to WebFetch / WebBrowser tool labels. (Lucide) */
function PaperclipIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551" />
    </svg>
  );
}

export default App;
