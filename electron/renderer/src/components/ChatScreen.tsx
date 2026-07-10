import { useCallback, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useChatStore } from '../stores/chat';
import { useSettingsStore, PROVIDER_TEMPLATES } from '../stores/settings';
import { buildHistoricalTurns, buildLiveSegments } from '../utils/timeline';
import type { TimelineGroup } from '../types/timeline';
import { ChatView, ModelOption } from './ChatView';
import { ChatInputPanel } from './ChatInputPanel';

/** Chat screen: orchestrates messages + input. Owns compact state and
 *  computes timeline. Renders ChatView (pure) and ChatInputPanel (memoised). */
export function ChatScreen() {
  const { t } = useTranslation();
  const {
    sessions, activeSessionId, error, clearError, notice, setNotice,
    compactSession, compactResult, clearCompactResult, createSession,
  } = useChatStore();
  const { config } = useSettingsStore();
  const [compacting, setCompacting] = useState(false);
  const inputRef = useRef<{ send: () => void }>(null);

  const activeSession = sessions.find((s) => s.id === activeSessionId);
  const currentTurn = activeSession?.currentTurn || [];
  const isStreaming = activeSession?.isStreaming || false;
  const thinkingEnabled: boolean = activeSession?.thinking_enabled ?? true;

  const modelOptions: ModelOption[] = useMemo(() => {
    if (!config) return [];
    const opts: ModelOption[] = [];
    for (const p of config.llm.providers) {
      const tmpl = PROVIDER_TEMPLATES[p.id];
      const name = tmpl?.name || p.id;
      for (const m of p.models) {
        opts.push({ providerId: p.id, providerName: name, model: m, key: `${p.id}/${m}` });
      }
    }
    return opts;
  }, [config]);

  const selectedModelKey = activeSession?.model_key || config?.llm?.default_model_id || modelOptions[0]?.key || '';

  const historicalGroups = useMemo(
    () => (activeSession ? buildHistoricalTurns(activeSession.messages) : []),
    [activeSession?.messages],
  );
  const live = useMemo(
    () => (currentTurn.length > 0 ? buildLiveSegments(currentTurn) : null),
    [currentTurn],
  );
  const timeline: TimelineGroup[] = useMemo(() => [
    ...historicalGroups,
    ...(live ? [{ kind: 'liveTurn' as const, id: 'live-turn', segments: live.segments, isStreaming: live.isStreaming }] : []),
  ], [historicalGroups, live]);

  const handleSelectModel = useCallback((key: string) => {
    if (activeSessionId) useChatStore.getState().setSessionModel(activeSessionId, key);
  }, [activeSessionId]);

  const handleToggleThinking = useCallback(() => {
    if (!activeSessionId) return;
    const cur = useChatStore.getState().sessions.find((s) => s.id === activeSessionId);
    useChatStore.getState().setSessionThinking(activeSessionId, !(cur?.thinking_enabled ?? true));
  }, [activeSessionId]);

  const handleSelectWorkspace = useCallback((wsPath: string) => {
    if (!activeSessionId) return;
    useChatStore.getState().setSessionWorkspace(activeSessionId, wsPath);
  }, [activeSessionId]);

  const handleCompact = useCallback(async () => {
    if (!activeSessionId) return;
    setCompacting(true);
    try {
      const result = await compactSession(activeSessionId);
      if (result.compacted) {
        useChatStore.setState({
          compactResult: {
            kind: 'success',
            beforeTokens: result.beforeTokens,
            afterTokens: result.afterTokens,
          },
        });
      } else {
        useChatStore.setState({
          compactResult: {
            kind: 'error',
            reason: result.reason || t('compact.failed'),
          },
        });
      }
    } finally {
      setCompacting(false);
    }
  }, [activeSessionId, compactSession, t]);

  return (
    <ChatView
      timeline={timeline}
      error={error}
      onClearError={clearError}
      notice={notice}
      onClearNotice={() => setNotice(null)}
      compactResult={compactResult}
      onClearCompactResult={clearCompactResult}
      hasActiveSession={!!activeSession}
      onCreateSession={createSession}
      onRetry={() => inputRef.current?.send()}
      inputPanel={
        <ChatInputPanel
          imperativeRef={inputRef}
          sessionId={activeSessionId}
          isStreaming={isStreaming}
          thinkingEnabled={thinkingEnabled}
          onToggleThinking={handleToggleThinking}
          modelOptions={modelOptions}
          selectedModelKey={selectedModelKey}
          onSelectModel={handleSelectModel}
          workspaceDir={activeSession?.workspace_dir || ''}
          workspaceOptions={config?.workspaces || []}
          onSelectWorkspace={handleSelectWorkspace}
          onCompact={activeSessionId ? handleCompact : undefined}
          compactDisabled={compacting || isStreaming || !activeSessionId}
          compacting={compacting}
          messageCount={activeSession?.messages.length ?? 0}
        />
      }
    />
  );
}