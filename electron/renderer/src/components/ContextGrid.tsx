/**
 * ContextGrid — per-session context-window usage indicator.
 *
 * Renders a 3×3 mini-grid icon in the input area.  Clicking it opens a
 * popover with a 10×10 grid (each cell = 1% of the context window).
 *
 * Colours (from left-to-right, top-to-bottom):
 *   Dark gray  → system prompt (immutable per turn)
 *   Light gray → message history
 *   White      → unused headroom
 *
 * Below the grid a legend shows "上下文已占用 X%（其中系统提示词占 Y%）".
 */
import { useEffect, useState, useCallback, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useChatStore } from '../stores/chat';
import { CompressIcon } from './icons';

// ── Types ──────────────────────────────────────────────────────────

interface ContextStats {
  systemTokens: number;
  messageTokens: number;
  contextWindow: number;
  compacted: boolean;
}

interface Props {
  sessionId: string | null;
  modelKey?: string;
  workspaceDir?: string;
  /** Compact handlers — render a compress button next to the summary
   *  text inside the popover. Mirrors the logic that previously lived
   *  as a standalone icon in the input area. */
  onCompact?: () => void;
  compactDisabled?: boolean;
  compacting?: boolean;
  messageCount?: number;
  /** Whether the current turn is streaming — disables compact mid-stream. */
  streaming?: boolean;
}

// ── Mini-icon (3×3 preview) ────────────────────────────────────────
//
// Sized to match the surrounding 26×26 icon buttons (Compress / Upload).
// 4×4 cells × 3 + 1.5px gap × 2 = 15×15 visual size, centered with no padding.
function MiniGridIcon({ filled }: { filled: number }) {
  return (
    <div
      style={{
        display: 'inline-flex',
        gap: '1.5px',
        flexDirection: 'column',
        lineHeight: 0, // prevent inline-flex from adding descender space
      }}
    >
      {[0, 1, 2].map(row => (
        <div key={row} style={{ display: 'flex', gap: '1.5px' }}>
          {[0, 1, 2].map(col => {
            const cellIdx = row * 3 + col;
            const on = cellIdx / 9 * 100 < filled;
            return (
              <span
                key={col}
                style={{
                  width: '4px',
                  height: '4px',
                  borderRadius: '1px',
                  background: on ? '#555' : '#ddd',
                  transition: 'background 0.2s',
                  display: 'block',
                }}
              />
            );
          })}
        </div>
      ))}
    </div>
  );
}

// ── Helpers ────────────────────────────────────────────────────────

function clamp(v: number, min: number, max: number) { return Math.max(min, Math.min(max, v)); }

// ── Component ──────────────────────────────────────────────────────

export function ContextGrid({
  sessionId, modelKey, workspaceDir,
  onCompact, compactDisabled, compacting, messageCount, streaming,
}: Props) {
  const { t } = useTranslation();
  const [stats, setStats] = useState<ContextStats | null>(null);
  const [open, setOpen] = useState(false);
  const [hover, setHover] = useState(false);
  const [compactHover, setCompactHover] = useState(false);
  const wrapperRef = useRef<HTMLDivElement | null>(null);

  // Fetch stats.
  const refresh = useCallback(async () => {
    if (!sessionId) return;
    try {
      const s = await window.electronAPI.session.contextStats(sessionId, modelKey, workspaceDir);
      setStats(s);
    } catch {
      // Silently ignore — the grid just stays stale.
    }
  }, [sessionId, modelKey, workspaceDir]);

  // Refresh on mount + when session / model / workspace changes.
  useEffect(() => { refresh(); }, [refresh]);

  // Also refresh when chat store signals session data changed
  // (e.g. after a `compacted` event finishes, or `done` reloads the session).
  const statsVersion = useChatStore((s) => s.statsVersion);
  useEffect(() => { refresh(); }, [refresh, statsVersion]);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', h);
    return () => document.removeEventListener('mousedown', h);
  }, [open]);

  // Close when the mouse leaves the wrapper. The popup floats 6px above the
  // button, so debounce to avoid flicker while moving between the two. We use
  // a generous 1s window — short enough to feel passive, long enough that
  // glancing away (e.g. at the chat scrollbar) doesn't snap it shut.
  useEffect(() => {
    if (!open) return;
    const el = wrapperRef.current;
    if (!el) return;
    let leaveTimer: number | undefined;
    const onEnter = () => {
      if (leaveTimer !== undefined) {
        window.clearTimeout(leaveTimer);
        leaveTimer = undefined;
      }
    };
    const onLeave = () => {
      leaveTimer = window.setTimeout(() => setOpen(false), 600);
    };
    el.addEventListener('mouseenter', onEnter);
    el.addEventListener('mouseleave', onLeave);
    return () => {
      el.removeEventListener('mouseenter', onEnter);
      el.removeEventListener('mouseleave', onLeave);
      if (leaveTimer !== undefined) window.clearTimeout(leaveTimer);
    };
  }, [open]);

  // Escape key dismisses the popup.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open]);

  // ── Derived values ─────────────────────────────────────────────
  const derived = useMemo(() => {
    if (!stats || stats.contextWindow <= 0) return null;
    const total = stats.systemTokens + stats.messageTokens;
    const win = stats.contextWindow;
    const systemPct = clamp(Math.round((stats.systemTokens / win) * 100), 0, 100);
    const msgPct = clamp(Math.round((stats.messageTokens / win) * 100), 0, 100);
    const freePct = clamp(100 - systemPct - msgPct, 0, 100);
    return { systemPct, msgPct, freePct, totalPct: systemPct + msgPct, total, win, compacted: stats.compacted };
  }, [stats]);

  const gridFill = derived ? derived.totalPct : 0;

  // ── Render ──────────────────────────────────────────────────────
  if (!sessionId) return null;

  return (
    <div className="ctx-wrapper" ref={wrapperRef} style={{ position: 'relative', display: 'inline-flex' }}>
      <button
        type="button"
        className="ctx-mini-btn"
        onClick={() => { setOpen(v => !v); if (!stats) refresh(); }}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        title={t('context.title')}
        style={{
          appearance: 'none',
          WebkitAppearance: 'none',
          background: open ? '#e8e8e8' : (hover ? '#f3f3f3' : 'transparent'),
          color: '#888',
          border: '1px solid transparent',
          borderRadius: '6px',
          width: '26px',
          height: '26px',
          padding: 0,
          font: 'inherit',
          cursor: 'pointer',
          display: 'inline-flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexShrink: 0,
          transition: 'background 0.12s',
        }}
      >
        <MiniGridIcon filled={gridFill} />
      </button>

      {open && (
        <div className="ctx-popup">
          {/* 10×10 grid */}
          <div className="ctx-grid-10x10">
            {Array.from({ length: 100 }).map((_, i) => {
              let color = '#fafafa'; // very light gray — visible against white popover
              if (derived) {
                if (i < derived.systemPct) color = '#4a4a4f';              // dark gray = system prompt
                else if (i < derived.systemPct + derived.msgPct) color = '#d4d4d4'; // light gray = messages
              }
              return (
                <span key={i} className="ctx-cell" style={{ background: color }} />
              );
            })}
          </div>

          {/* Legend */}
          <div className="ctx-legend">
            <div className="ctx-legend-row">
              <span className="ctx-swatch" style={{ background: '#4a4a4f' }} />
              <span>{t('context.systemPrompt')}</span>
              <span className="ctx-pct">{derived?.systemPct ?? '--'}%</span>
            </div>
            <div className="ctx-legend-row">
              <span className="ctx-swatch" style={{ background: '#d4d4d4' }} />
              <span>{t('context.messageHistory')}</span>
              <span className="ctx-pct">{derived?.msgPct ?? '--'}%</span>
            </div>
            <div className="ctx-legend-row">
              <span className="ctx-swatch" style={{ background: '#fafafa', border: '1px solid #e5e5e5' }} />
              <span>{t('context.remaining')}</span>
              <span className="ctx-pct">{derived?.freePct ?? '--'}%</span>
            </div>
          </div>

          <div className="ctx-summary-row">
            <span className="ctx-summary-text">
              {derived
                ? t('context.summary', { totalPct: derived.totalPct })
                : t('context.loading')}
            </span>
            {onCompact && (() => {
              const tooFew = (messageCount ?? 0) < 6;
              const fullyDisabled = compactDisabled || streaming || tooFew || compacting;
              const tooltip = compacting
                ? t('compact.pending')
                : streaming
                  ? t('compact.streamingDisabled')
                  : tooFew
                    ? t('compact.tooFew', { min: 6, current: messageCount })
                    : t('compact.title');
              const label = compacting ? t('compact.pending') : t('compact.button');
              return (
                <button
                  type="button"
                  className="ctx-compact-btn"
                  disabled={fullyDisabled}
                  title={tooltip}
                  onClick={onCompact}
                  onMouseEnter={() => setCompactHover(true)}
                  onMouseLeave={() => setCompactHover(false)}
                  style={{
                    appearance: 'none', WebkitAppearance: 'none',
                    background: compactHover && !fullyDisabled ? '#f3f3f3' : 'transparent',
                    color: tooFew ? '#bbb' : '#555',
                    border: '1px solid transparent',
                    borderRadius: '5px',
                    height: '22px',
                    padding: '0 8px',
                    display: 'inline-flex',
                    alignItems: 'center', justifyContent: 'center',
                    gap: '4px',
                    font: 'inherit',
                    fontSize: '12px',
                    cursor: fullyDisabled ? 'not-allowed' : 'pointer',
                    opacity: fullyDisabled ? 0.5 : 1,
                    flexShrink: 0,
                    transition: 'background 0.12s',
                  }}
                >
                  {compacting ? <span className="compact-spinner" /> : <CompressIcon size={13} />}
                  <span>{label}</span>
                </button>
              );
            })()}
          </div>
        </div>
      )}
    </div>
  );
}
