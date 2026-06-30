import { useEffect, useRef, useState } from 'react';

interface Props {
  suggested: string[];
  placeholder: string;
  onPick: (model: string) => void;
}

/**
 * A small custom dropdown used in place of <select> for the "Suggest" picker.
 * macOS Chrome sometimes renders native <select> text in a system font/size
 * that ignores CSS, which makes the placeholder appear too large and clips
 * it. Rendering our own button + popup keeps the look consistent across
 * locales (zh, en, ja, ko, etc.).
 */
export function SuggestPicker({ suggested, placeholder, onPick }: Props) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  return (
    <span className="suggest-picker" ref={wrapRef}>
      <button
        type="button"
        className="suggest-picker-btn"
        onClick={() => setOpen(o => !o)}
      >
        <span>{placeholder}</span>
        <span className="suggest-picker-chevron" aria-hidden="true">
          <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </span>
      </button>
      {open && (
        <ul className="suggest-picker-menu">
          {suggested.map(m => (
            <li
              key={m}
              className="suggest-picker-item"
              onClick={() => { setOpen(false); onPick(m); }}
            >
              {m}
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}
