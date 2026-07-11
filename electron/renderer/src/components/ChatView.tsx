import { useEffect, useRef } from 'react';
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

export function ChatView({ timeline, activeSessionId, error, onClearError, notice, onClearNotice, compactResult, onClearCompactResult, hasActiveSession, onCreateSession, onRetry, inputPanel }: Props) {
  const { t } = useTranslation();
  const messagesRef = useRef<HTMLDivElement>(null);
  // True when user is at (or near) the bottom. While streaming we only
  // auto-scroll if this is true, so users can scroll up to read history
  // without being yanked back. Reset on session switch (handled below).
  const userAtBottomRef = useRef(true);
  const STICK_THRESHOLD = 32; // px from bottom counted as "still at bottom"

  const updateStickiness = () => {
    const el = messagesRef.current;
    if (!el) return;
    const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    userAtBottomRef.current = distFromBottom <= STICK_THRESHOLD;
  };

  // Session switch — reset stickiness and always jump to bottom.
  useEffect(() => {
    const el = messagesRef.current;
    if (!el) return;
    userAtBottomRef.current = true;
    el.scrollTop = el.scrollHeight;
  }, [activeSessionId]);

  // Timeline update — only follow if user is already at the bottom.
  useEffect(() => {
    const el = messagesRef.current;
    if (!el) return;
    if (userAtBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [timeline]);

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
            {timeline.map((group) => {
              if (group.kind === 'user') return <UserMessageView key={group.id} content={group.content} images={group.images} />;
              if (group.kind === 'liveTurn') return <LiveTurnView key="live-turn" segments={group.segments} isStreaming={group.isStreaming} />;
              return <AgentTurnView key={`${group.id}-done`} segments={group.segments} conclusion={group.conclusion} />;
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