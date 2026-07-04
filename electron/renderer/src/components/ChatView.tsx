import { useTranslation } from 'react-i18next';
import { UserMessageView } from './UserMessageView';
import { LiveTurnView } from './LiveTurn';
import { AgentTurnView } from './AgentTurn';
import { InputArea } from './InputArea';
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
  error: ChatError | null;
  onClearError: () => void;
  hasActiveSession: boolean;
  onCreateSession: () => void;
  input: {
    value: string;
    onChange: (v: string) => void;
    onSend: () => void;
    disabled: boolean;
    sendDisabled?: boolean;
    textareaRef: React.RefObject<HTMLTextAreaElement | null>;
    onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
    thinkingEnabled: boolean;
    onToggleThinking: () => void;
  };
  streaming: boolean;
  onCancel: () => void;
  modelOptions: ModelOption[];
  selectedModelKey: string;
  onSelectModel: (key: string) => void;
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

export function ChatView({ timeline, error, onClearError, hasActiveSession, onCreateSession, input, streaming, onCancel, modelOptions, selectedModelKey, onSelectModel }: Props) {
  const { t } = useTranslation();

  return (
    <main className="chat-area">
      {error && (
        <div className={`error error--${error.errorCode.toLowerCase()}`}>
          <span className="error-message">{error.message}</span>
          {error.retryable && <button className="error-retry" onClick={(e) => { e.stopPropagation(); input.onSend(); }}>{t('retry')}</button>}
          {!error.retryable && errorAction(error.errorCode) && (
            <span className="error-action">{errorAction(error.errorCode)}</span>
          )}
          <button className="error-dismiss" onClick={onClearError} aria-label={t('close')}>✕</button>
        </div>
      )}

      {hasActiveSession ? (
        <>
          <div className="messages">
            {timeline.map((group) => {
              if (group.kind === 'user') return <UserMessageView key={group.id} content={group.content} />;
              if (group.kind === 'liveTurn') return <LiveTurnView key="live-turn" segments={group.segments} isStreaming={group.isStreaming} />;
              return <AgentTurnView key={`${group.id}-done`} segments={group.segments} conclusion={group.conclusion} />;
            })}
          </div>
          <InputArea
            {...input}
            streaming={streaming}
            onCancel={onCancel}
            modelOptions={modelOptions}
            selectedModelKey={selectedModelKey}
            onSelectModel={onSelectModel}
          />
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