/**
 * ModelSelect — custom dropdown for the input-area model picker.
 *
 * Why a custom component? Native `<select>` has two properties we can't
 * get around with CSS alone:
 *  1. The trigger width is determined by either the longest option or a
 *     fixed pixel value — it doesn't auto-size to the *selected* label.
 *  2. The dropdown panel width is controlled by the browser (usually
 *     sized to the widest option), so it visually detaches from the
 *     trigger.
 *
 * This component:
 *  - Width is driven by the *selected* option's text + chevron (so a
 *    short model name like "M3" produces a tiny trigger, while "M3-very-
 *    long-variant" expands naturally).
 *  - The dropdown panel width matches the trigger.
 *  - Click-outside, Esc, and Arrow keys are handled.
 */
import { useEffect, useRef, useState } from 'react';

export interface ModelOption {
  providerId: string;
  providerName: string;
  model: string;
  key: string;
}

interface Props {
  options: ModelOption[];
  value: string;
  onChange: (key: string) => void;
  disabled?: boolean;
  placeholder?: string;
  title?: string;
}

function labelOf(opt: ModelOption): string {
  return opt.providerId === 'custom' ? `${opt.providerName} / ${opt.model}` : opt.model;
}

export function ModelSelect({ options, value, onChange, disabled, placeholder, title }: Props) {
  const [open, setOpen] = useState(false);
  const [hover, setHover] = useState(false);
  const [activeIdx, setActiveIdx] = useState(-1);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const [triggerWidth, setTriggerWidth] = useState<number>(0);

  const selected = options.find(o => o.key === value);
  const displayLabel = selected ? labelOf(selected) : (placeholder || '');

  // Measure the trigger so the dropdown panel can match it.
  useEffect(() => {
    if (!buttonRef.current) return;
    const measure = () => {
      if (buttonRef.current) setTriggerWidth(buttonRef.current.offsetWidth);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(buttonRef.current);
    return () => ro.disconnect();
  }, []);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  // Keyboard: Esc closes, ↑/↓ moves, Enter selects.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { setOpen(false); return; }
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIdx(i => Math.min(options.length - 1, i + 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIdx(i => Math.max(0, i - 1));
      } else if (e.key === 'Enter' && activeIdx >= 0) {
        e.preventDefault();
        onChange(options[activeIdx].key);
        setOpen(false);
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, activeIdx, options, onChange]);

  const noOptions = options.length === 0;
  const isDisabled = !!disabled || noOptions;

  return (
    <div className="model-select" ref={wrapperRef}>
      <button
        ref={buttonRef}
        type="button"
        className="model-select-trigger"
        onClick={() => !isDisabled && setOpen(v => !v)}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        disabled={isDisabled}
        title={title}
        style={{
          background: hover && !isDisabled ? '#f0f0f0' : 'transparent',
          color: noOptions ? '#bbb' : '#555',
          opacity: isDisabled ? 0.5 : 1,
          cursor: isDisabled ? 'not-allowed' : 'pointer',
        }}
      >
        <span className="model-select-label">{displayLabel || '\u00A0'}</span>
        <svg className="model-select-chevron" width="11" height="11" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <polyline points="3,5 6,8 9,5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>

      {open && !isDisabled && (
        <div
          className="model-select-dropdown"
          style={{ minWidth: triggerWidth ? `${triggerWidth}px` : '120px' }}
          role="listbox"
        >
          {options.map((opt, i) => {
            const isSelected = opt.key === value;
            const isActive = i === activeIdx;
            return (
              <div
                key={opt.key}
                role="option"
                aria-selected={isSelected}
                className={
                  'model-select-option'
                  + (isSelected ? ' selected' : '')
                  + (isActive ? ' active' : '')
                }
                onMouseEnter={() => setActiveIdx(i)}
                onClick={() => { onChange(opt.key); setOpen(false); }}
              >
                {labelOf(opt)}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}