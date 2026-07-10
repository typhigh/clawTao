import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { normalizeMd } from '../utils/format';
import { markdownComponents } from '../utils/markdown';
import type { TurnSegment } from '../types/timeline';
import { Thinking } from './Thinking';
import { TodoView } from './TodoView';
import { CompressIcon } from './icons';
import { ToolPairView } from './ToolCard';

/** Live turn: flat chronological segments, no fold. */
export function LiveTurnView({ segments, isStreaming }: { segments: TurnSegment[]; isStreaming: boolean }) {
  return (
    <div className={`agent-turn live ${isStreaming ? 'streaming' : ''}`}>
      {segments.map((seg, i) => {
        if (seg.kind === 'text') {
          return <div key={i} className="turn-text"><ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>{normalizeMd(seg.content)}</ReactMarkdown></div>;
        }
        if (seg.kind === 'thinking') {
          return <Thinking key={i} content={seg.content} forceOpen={isStreaming} />;
        }
        if (seg.kind === 'todo') {
          return <TodoView key={i} todos={seg.todos} />;
        }
        if (seg.kind === 'compacted') {
          return (
            <div key={i} className="turn-compacted-banner">
              <div className="turn-compacted-row">
                <span className="turn-compacted-icon"><CompressIcon /></span>
                <span className="turn-compacted-text">
                  Context compacted — earlier messages summarized
                  {seg.messageCount != null && ` (${seg.messageCount} messages remaining)`}.
                </span>
              </div>
              {seg.warning && <div className="turn-compacted-warning">⚠ {seg.warning}</div>}
            </div>
          );
        }
        return <ToolPairView key={seg.id} segment={seg} />;
      })}
    </div>
  );
}
