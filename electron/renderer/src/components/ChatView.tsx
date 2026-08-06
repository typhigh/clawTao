import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { UserMessageView } from './UserMessageView';
import { LiveTurnView } from './LiveTurn';
import { AgentTurnView } from './AgentTurn';
import type { TimelineGroup } from '../types/timeline';
import type { ChatError } from '../stores/chat';

export interface ModelOption {
  providerId: string;
  providerName: string;
  model: string;
  key: string;
}

interface Props {
  timeline: TimelineGroup[];
  /** Active session id — used to reset scroll-to-bottom stickiness on switch. */
  activeSessionId: string | null;
  error: ChatError | null;
  onClearError: () => void;
  /** Transient toast notice (auto-clears). */
  notice: { message: string; type: 'success' | 'info' | 'error' } | null;
  onClearNotice: () => void;
  hasActiveSession: boolean;
  onCreateSession: () => void;
  /** Trigger a retry from the error banner. */
  onRetry: () => void;
  /** Persistent compaction result — stays until dismissed or next send. */
  compactResult?: import('../stores/chat').CompactResult | null;
  onClearCompactResult?: () => void;
  /** Input panel as a slot — keeps the original layout (messages/scroll/input inside the same flex column). */
  inputPanel?: React.ReactNode;
  /** Per-turn generated files for historical turns (index-aligned with agent turns). */
  historicalFiles?: import('../lib/generated-files').GeneratedFile[][];
  /** Generated files for the live turn (null when no live turn or no files). */
  liveFiles?: import('../lib/generated-files').GeneratedFile[] | null;
  /** Called when a file change card is clicked. */
  onFileClick?: (file: import('../lib/generated-files').GeneratedFile) => void;
}

/** Format a token count for human display: 128432 → "128K", 2400 → "2.4K", 756 → "756". */
function fmtTokens(n: number): string {
  if (n < 1000) return String(n);
  const k = n / 1000;
  return k >= 100 ? `${Math.round(k)}K` : `${Math.round(k * 10) / 10}K`;
}

/** Actions the user can take for non-retryable errors. */
function errorAction(errorCode: string): string | null {
  switch (errorCode) {
    case 'UNAUTHORIZED':
    case 'CONFIG_ERROR':
      return 'Open Settings';
    case 'CONTEXT_EXCEEDED':
    case 'SESSION_ERROR':
      return 'New Session';
    default:
      return null;
  }
}

export function ChatView({ timeline, activeSessionId, error, onClearError, notice, onClearNotice, compactResult, onClearCompactResult, hasActiveSession, onCreateSession, onRetry, historicalFiles, liveFiles, onFileClick, inputPanel }: Props) {
  const { t } = useTranslation();
  const messagesRef = useRef<HTMLDivElement>(null);
  // True when user is at (or near) the bottom. While streaming we only
  // auto-scroll if this is true, so users can scroll up to read history
  // without being yanked back. Reset on session switch (handled below).
  const userAtBottomRef = useRef(true);
  const STICK_THRESHOLD = 32; // px from bottom counted as "still at bottom"

  // ── Pagination ──────────────────────────────────────────────────
  // Long-history sessions can have hundreds of turns; rendering them all
  // at once is the dominant cost of switching sessions. We render only
  // the last PAGE_SIZE *conversational turns* (a turn = one user message
  // + the matching agent response), with a "load earlier" button that
  // reveals PAGE_SIZE more each click. One turn ≈ one "I asked, you
  // answered" exchange from the user's perspective.
  const PAGE_SIZE = 10;
  const [visibleTurns, setVisibleTurns] = useState(PAGE_SIZE);
  // Absolute agent-turn prefix counts — O(n) once per timeline, replaces
  // the O(n²) `slice().filter()` per rendered group.
  const agentTurnPrefix = useMemo(() => {
    const prefix = new Array<number>(timeline.length);
    let count = 0;
    for (let i = 0; i < timeline.length; i++) {
      if (timeline[i].kind === 'agentTurn') count++;
      prefix[i] = count;
    }
    return prefix;
  }, [timeline]);
  // Map "visible turns from the tail" to a group slice. A turn is
  // anchored at the agent turn; we include the user message that
  // immediately precedes it (if any) so the response never appears
  // without its question. The liveTurn at the tail is always included.
  const visibleStartIdx = useMemo(() => {
    if (timeline.length === 0) return 0;
    const totalAgentTurns = agentTurnPrefix[timeline.length - 1] || 0;
    if (visibleTurns >= totalAgentTurns && timeline[timeline.length - 1]?.kind !== 'liveTurn') {
      return 0; // everything fits
    }
    // Walk from the tail, counting agent turns, until we hit `visibleTurns`.
    // Then walk one step back to include the matching user message (if any).
    let turnsSeen = 0;
    for (let i = timeline.length - 1; i >= 0; i--) {
      if (timeline[i].kind === 'agentTurn' || timeline[i].kind === 'liveTurn') {
        turnsSeen++;
        if (turnsSeen > visibleTurns) {
          // i is the first group *past* the window; include the preceding
          // user message (if present) so the turn isn't orphaned.
          let start = i + 1;
          if (start < timeline.length && timeline[start].kind === 'liveTurn') start++;
          if (start > 0 && timeline[start - 1].kind === 'user') start--;
          return start;
        }
      }
    }
    return 0;
  }, [timeline, visibleTurns, agentTurnPrefix]);
  // Visible slice, keeping absolute index.
  const visible = useMemo(() => {
    const out: { group: TimelineGroup; idx: number }[] = [];
    for (let i = visibleStartIdx; i < timeline.length; i++) out.push({ group: timeline[i], idx: i });
    return out;
  }, [timeline, visibleStartIdx]);

  // Scroll-preservation machinery.
  // - prevScrollHeightRef: captured just before "load earlier" grows the
  //   top of the list; after the re-render we add the delta so the view
  //   doesn't jump.
  // - pendingSwitchScrollRef: set when the active session changes; we
  //   then scroll to bottom AFTER the pagination reset re-renders.
  const prevScrollHeightRef = useRef<number | null>(null);
  const lastSessionRef = useRef<string | null>(null);
  const pendingSwitchScrollRef = useRef(false);

  const updateStickiness = () => {
    const el = messagesRef.current;
    if (!el) return;
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    userAtBottomRef.current = distFromBottom <= STICK_THRESHOLD;
  };

  // Session switch — reset pagination to the newest PAGE_SIZE turns and
  // remember to scroll to bottom once the reset has rendered.
  useEffect(() => {
    if (lastSessionRef.current !== activeSessionId) {
      lastSessionRef.current = activeSessionId;
      userAtBottomRef.current = true;
      setVisibleTurns(PAGE_SIZE);
      pendingSwitchScrollRef.current = true;
    }
  }, [activeSessionId]);

  // After every commit, either finish a pending session-switch scroll or
  // preserve the viewport position after "load earlier" grew the top.
  useLayoutEffect(() => {
    const el = messagesRef.current;
    if (!el) return;
    if (pendingSwitchScrollRef.current) {
      pendingSwitchScrollRef.current = false;
      el.scrollTop = el.scrollHeight;
      return;
    }
    if (prevScrollHeightRef.current !== null) {
      el.scrollTop += el.scrollHeight - prevScrollHeightRef.current;
      prevScrollHeightRef.current = null;
    }
  });

  // Timeline update — only follow if user is already at the bottom.
  useEffect(() => {
    const el = messagesRef.current;
    if (!el) return;
    if (userAtBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [timeline]);

  const loadEarlier = () => {
    const el = messagesRef.current;
    if (el) prevScrollHeightRef.current = el.scrollHeight;
    const totalTurns = agentTurnPrefix[timeline.length - 1] || 0;
    setVisibleTurns((c) => Math.min(totalTurns, c + PAGE_SIZE));
  };
  const totalTurns = agentTurnPrefix[timeline.length - 1] || 0;
  const hasEarlier = visibleTurns < totalTurns;
  const earlierCount = Math.min(PAGE_SIZE, totalTurns - visibleTurns);

  return (
    <main className="chat-area">
      {error && (
        <div className={`error error--${error.errorCode.toLowerCase()}`}>
          <span className="error-message">{error.message}</span>
          {error.retryable && <button className="error-retry" onClick={(e) => { e.stopPropagation(); onRetry(); }}>{t('retry')}</button>}
          {!error.retryable && errorAction(error.errorCode) && (
            <span className="error-action">{errorAction(error.errorCode)}</span>
          )}
          <button className="error-dismiss" onClick={onClearError} aria-label={t('close')}>✕</button>
        </div>
      )}

      {notice && (
        <div className={`notice notice--${notice.type}`}>
          <span className="notice-message">{notice.message}</span>
          <button className="notice-dismiss" onClick={onClearNotice} aria-label={t('close')}>✕</button>
        </div>
      )}

      {compactResult && (
        <div className={`compact-result compact-result--${compactResult.kind}`}>
          <span className="compact-result-icon">{compactResult.kind === 'success' ? '✓' : '✗'}</span>
          <span className="compact-result-message">
            {compactResult.kind === 'success'
              ? (compactResult.beforeTokens != null && compactResult.afterTokens != null && compactResult.beforeTokens > 0
                ? t('compact.bannerSuccess', {
                    before: fmtTokens(compactResult.beforeTokens),
                    after: fmtTokens(compactResult.afterTokens),
                    saved: Math.max(0, Math.round(((compactResult.beforeTokens - compactResult.afterTokens) / compactResult.beforeTokens) * 100)),
                  })
                : t('compact.bannerSuccessFallback'))
              : t('compact.bannerFailed', { reason: compactResult.reason || t('compact.failed') })}
          </span>
          <button className="compact-result-dismiss" onClick={onClearCompactResult} aria-label={t('close')}>✕</button>
        </div>
      )}

      {hasActiveSession ? (
        <>
          <div className="messages" ref={messagesRef} onScroll={updateStickiness}>
            {hasEarlier && (
              <button type="button" className="load-earlier" onClick={loadEarlier}>
                {t('chat.loadEarlier', { count: earlierCount })}
              </button>
            )}
            {visible.map(({ group, idx }) => {
              if (group.kind === 'user') return <UserMessageView key={group.id} content={group.content} images={group.images} />;
              if (group.kind === 'liveTurn') return <LiveTurnView key="live-turn" segments={group.segments} isStreaming={group.isStreaming} files={liveFiles ?? undefined} onFileClick={onFileClick} />;
              // agent turn → its zero-based index among agent turns in the
              // FULL timeline (historicalFiles is indexed the same way).
              const agentTurnIdx = agentTurnPrefix[idx] - 1;
              const turnFiles = historicalFiles?.[agentTurnIdx];
              return <AgentTurnView key={`${group.id}-done`} segments={group.segments} conclusion={group.conclusion} files={turnFiles} onFileClick={onFileClick} />;
            })}
          </div>
          {timeline.length > 0 && (
            <div className="scroll-controls">
              <button
                type="button"
                className="scroll-btn"
                onClick={() => { if (messagesRef.current) messagesRef.current.scrollTop = 0; }}
                title={t('chat.scrollToTop')}
                aria-label={t('chat.scrollToTop')}
              >↑</button>
              <button
                type="button"
                className="scroll-btn"
                onClick={() => { if (messagesRef.current) messagesRef.current.scrollTop = messagesRef.current.scrollHeight; }}
                title={t('chat.scrollToBottom')}
                aria-label={t('chat.scrollToBottom')}
              >↓</button>
            </div>
          )}
          {inputPanel}
        </>
      ) : (
        <div className="empty-state">
          <p>{t('chat.noSession')}</p>
          <button onClick={onCreateSession}>{t('chat.createSession')}</button>
        </div>
      )}
    </main>
  );
}