import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MarkdownSegment } from '../utils/markdown';

/** Thinking block — collapsible card. Expanded while streaming (forceOpen),
 *  collapsed for historical turns. Always shown (thinking is hardcoded on). */
export function Thinking({ content, forceOpen = false }: { content: string; forceOpen?: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(forceOpen);
  useEffect(() => { setOpen(forceOpen); }, [forceOpen]);

  return (
    <div className={`thinking-block ${open ? 'open' : ''}`}>
      <button type="button" className="thinking-header" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span className="thinking-title">{t('thinking.title')}</span>
        <span className={`thinking-chevron ${open ? 'open' : ''}`}>›</span>
      </button>
      {open && (
        <div className="thinking-body">
          <MarkdownSegment content={content} />
        </div>
      )}
    </div>
  );
}
