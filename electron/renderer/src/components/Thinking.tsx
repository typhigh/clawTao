import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { normalizeMd } from '../utils/format';
import { markdownComponents } from '../utils/markdown';

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
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{normalizeMd(content)}</ReactMarkdown>
        </div>
      )}
    </div>
  );
}
