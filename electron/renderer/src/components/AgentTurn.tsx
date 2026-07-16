import { memo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { normalizeMd } from '../utils/format';
import { markdownComponents } from '../utils/markdown';
import { pairToolWithResults, countTurnSegments } from '../utils/timeline';
import type { AssistantSegment } from '../types/timeline';
import type { GeneratedFile } from '../lib/generated-files';
import { SegmentView } from './ToolCard';
import { FileChangesPanel } from './FileChangesPanel';

/** Historical turn: tools folded, conclusion always visible. */
function AgentTurnViewInner({
  segments,
  conclusion,
  files,
  onFileClick,
}: {
  segments: AssistantSegment[];
  conclusion: string | null;
  files?: GeneratedFile[];
  onFileClick?: (file: GeneratedFile) => void;
}) {
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
          {processSegments.map((seg) => <SegmentView key={seg.id} segment={seg} />)}
        </div>
      )}
      {conclusion && (
        <div className="agent-turn-conclusion">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{normalizeMd(conclusion)}</ReactMarkdown>
        </div>
      )}
      {files && files.length > 0 && onFileClick && (
        <FileChangesPanel files={files} onFileClick={onFileClick} />
      )}
    </div>
  );
}

export const AgentTurnView = memo(AgentTurnViewInner);
