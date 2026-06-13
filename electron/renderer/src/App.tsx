import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
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
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean };

type TurnSegment =
  | { kind: 'text'; content: string }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean };

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
        // If the model emitted text before issuing tool calls, store it
        // as a chronologically-placed text segment inside the turn.
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
      } else if (msg.content) {
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
  let pendingTool: { id: string; name: string; input: unknown } | null = null;

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
      case 'text':
        textBuf += ev.delta!;
        break;
      case 'tool_call':
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

  flushText();
  flushPending();

  return { segments, isStreaming: !hasDone };
}

// ── Segment pairing (for historical turns) ───────────────────────────

function pairToolWithResults(segments: AssistantSegment[]): AssistantSegment[] {
  const out: AssistantSegment[] = [];
  let lastTool: Extract<AssistantSegment, { kind: 'tool' }> | null = null;
  for (const s of segments) {
    if (s.kind === 'tool') {
      if (lastTool) {
        out.push({ kind: 'toolPair', id: lastTool.id, toolName: lastTool.toolName, toolInput: lastTool.toolInput, result: null, pending: true });
      }
      lastTool = s;
    } else if (s.kind === 'toolResult') {
      if (lastTool && lastTool.id === s.toolCallId) {
        out.push({ kind: 'toolPair', id: lastTool.id, toolName: lastTool.toolName, toolInput: lastTool.toolInput, result: s.content, pending: false });
        lastTool = null;
      } else {
        out.push(s);
      }
    } else {
      if (lastTool) {
        out.push({ kind: 'toolPair', id: lastTool.id, toolName: lastTool.toolName, toolInput: lastTool.toolInput, result: null, pending: true });
        lastTool = null;
      }
      out.push(s);
    }
  }
  if (lastTool) {
    out.push({ kind: 'toolPair', id: lastTool.id, toolName: lastTool.toolName, toolInput: lastTool.toolInput, result: null, pending: true });
  }
  return out;
}

function countTurnSegments(segments: AssistantSegment[]): { toolCount: number; processCount: number } {
  let toolCount = 0;
  let processCount = 0;
  for (const s of segments) {
    if (s.kind === 'tool' || s.kind === 'toolPair') toolCount++;
    else if (s.kind === 'toolResult') processCount++;
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
  // const initRef = useRef(false);

  useEffect(() => {
    loadSessions();
    loadConfig();
    window.electronAPI.onStreamEvent(handleStreamEvent);
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
    await useChatStore.getState().sendMessage(text);
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
                <input
                  type="text"
                  placeholder={t('chat.placeholder')}
                  value={inputValue}
                  onChange={(e) => setInputValue(e.target.value)}
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
      {segments.map((seg, i) =>
        seg.kind === 'text' ? (
          <div key={i} className="turn-text">{seg.content}</div>
        ) : (
          <ToolPairView key={seg.id} segment={seg} />
        ),
      )}
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
          <span className={`agent-turn-caret ${open ? 'open' : ''}`}>▸</span>
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
      {conclusion && <div className="agent-turn-conclusion">{conclusion}</div>}
    </div>
  );
}

function SegmentView({ segment }: { segment: AssistantSegment }) {
  const { t } = useTranslation();
  if (segment.kind === 'toolPair') {
    return (
      <div className={`turn-segment tool-pair ${segment.pending ? 'pending' : 'done'}`}>
        <div className="turn-segment-label">
          🔧 {segment.toolName}
          {segment.pending && <span className="turn-segment-spinner" />}
        </div>
        <div className="turn-segment-body tool-card">
          <div className="tool-input">
            <strong>{t('tool.input')}:</strong> {safeStringify(segment.toolInput)}
          </div>
          {segment.result !== null && (
            <>
              <div className="tool-result-divider" />
              <div className="tool-result">
                <strong>{t('tool.result')}:</strong>
                <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{segment.result}</pre>
              </div>
            </>
          )}
        </div>
      </div>
    );
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
  if (segment.kind === 'text') {
    return <div className="turn-segment turn-text">{segment.content}</div>;
  }
  return null;
}

/** Tool card used in the live turn (receives TurnSegment, already paired). */
function ToolPairView({ segment }: { segment: Extract<TurnSegment, { kind: 'toolPair' }> }) {
  const { t } = useTranslation();
  return (
    <div className={`turn-segment tool-pair ${segment.pending ? 'pending' : 'done'}`}>
      <div className="turn-segment-label">
        🔧 {segment.toolName}
        {segment.pending && <span className="turn-segment-spinner" />}
      </div>
      <div className="turn-segment-body tool-card">
        {segment.toolInput !== null && (
          <div className="tool-input">
            <strong>{t('tool.input')}:</strong> {safeStringify(segment.toolInput)}
          </div>
        )}
        {segment.result !== null && (
          <>
            {segment.toolInput !== null && <div className="tool-result-divider" />}
            <div className="tool-result">
              <strong>{t('tool.result')}:</strong>
              <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{segment.result}</pre>
            </div>
          </>
        )}
      </div>
    </div>
  );
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

export default App;
