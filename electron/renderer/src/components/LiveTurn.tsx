import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { normalizeMd } from '../utils/format';
import { markdownComponents } from '../utils/markdown';
import type { TurnSegment } from '../types/timeline';
import { Thinking } from './Thinking';
import { TodoView } from './TodoView';
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
        return <ToolPairView key={seg.id} segment={seg} />;
      })}
    </div>
  );
}
