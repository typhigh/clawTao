import { useTranslation } from 'react-i18next';
import { UserMessageView } from './UserMessageView';
import { LiveTurnView } from './LiveTurn';
import { AgentTurnView } from './AgentTurn';
import { InputArea } from './InputArea';
import type { TimelineGroup } from '../types/timeline';

interface Props {
  timeline: TimelineGroup[];
  error: string | null;
  onClearError: () => void;
  hasActiveSession: boolean;
  onCreateSession: () => void;
  input: {
    value: string;
    onChange: (v: string) => void;
    onSend: () => void;
    disabled: boolean;
    textareaRef: React.RefObject<HTMLTextAreaElement | null>;
    onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  };
}

export function ChatView({ timeline, error, onClearError, hasActiveSession, onCreateSession, input }: Props) {
  const { t } = useTranslation();

  return (
    <main className="chat-area">
      {error && <div className="error" onClick={onClearError}>{error}</div>}

      {hasActiveSession ? (
        <>
          <div className="messages">
            {timeline.map((group) => {
              if (group.kind === 'user') return <UserMessageView key={group.id} content={group.content} />;
              if (group.kind === 'liveTurn') return <LiveTurnView key="live-turn" segments={group.segments} isStreaming={group.isStreaming} />;
              return <AgentTurnView key={`${group.id}-done`} segments={group.segments} conclusion={group.conclusion} />;
            })}
          </div>
          <InputArea {...input} />
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
