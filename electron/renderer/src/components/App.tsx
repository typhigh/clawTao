import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useChatStore } from '../stores/chat';
import { useSettingsStore, PROVIDER_TEMPLATES } from '../stores/settings';
import { SettingsView } from './SettingsView';
import { Sidebar } from './Sidebar';
import { ChatView, ModelOption } from './ChatView';
import { buildHistoricalTurns, buildLiveSegments } from '../utils/timeline';
import type { TimelineGroup } from '../types/timeline';

type View = 'chat' | 'settings';

function App() {
  const { t } = useTranslation();
  const {
    sessions, activeSessionId,
    loadSessions, createSession, selectSession, deleteSession,
    error, clearError,
    handleStreamEvent,
  } = useChatStore();

  const { config, loaded, load: loadConfig } = useSettingsStore();
  const [view, setView] = useState<View>('chat');
  const [inputValue, setInputValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  useEffect(() => {
    loadSessions();
    loadConfig();
    window.electronAPI.onStreamEvent(handleStreamEvent);
    return () => { window.electronAPI.onStreamEvent(() => {}); };
  }, []);

  // First-run with no api_key anywhere → land in settings.
  useEffect(() => {
    if (loaded && config && (config.providers.length === 0 || config.providers.every(p => !p.api_key))) {
      setView('settings');
    }
  }, [loaded, config]);

  const modelOptions: ModelOption[] = useMemo(() => {
    if (!config) return [];
    const opts: ModelOption[] = [];
    for (const p of config.providers) {
      const tmpl = PROVIDER_TEMPLATES[p.id];
      const name = tmpl?.name || p.id;
      for (const m of p.models) {
        opts.push({ providerId: p.id, providerName: name, model: m, key: `${p.id}/${m}` });
      }
    }
    return opts;
  }, [config]);

  const handleSelectModel = async (key: string) => {
    if (!activeSessionId) return;
    useChatStore.getState().setSessionModel(activeSessionId, key);
  };

  const handleOpenSettings = () => {
    // Clear the active session highlight while in settings so the sidebar
    // shows settings as the only "active" entry.
    useChatStore.setState({ activeSessionId: null });
    setView('settings');
  };
  const handleBackToChat = () => setView('chat');

  // Send
  const handleSend = async () => {
    if (!inputValue.trim()) return;
    if (isStreaming) return; // refuse while a previous answer is still streaming
    const text = inputValue;
    setInputValue('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${2 * 24}px`;
    }
    await useChatStore.getState().sendMessage(text);
  };

  // Auto-grow textarea
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = 'auto';
    const minHeight = 2 * 24;
    const maxHeight = 6 * 24;
    const target = Math.max(minHeight, Math.min(ta.scrollHeight, maxHeight));
    ta.style.height = `${target}px`;
  }, [inputValue]);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      if (isStreaming) {
        e.preventDefault(); // swallow Enter — no send, no newline
        return;
      }
      e.preventDefault();
      handleSend();
    }
  };

  // Derived
  const activeSession = sessions.find((s) => s.id === activeSessionId);
  const currentTurn = activeSession?.currentTurn || [];
  const isStreaming = activeSession?.isStreaming || false;

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

  const selectedModelKey = activeSession?.model_key || config?.default_model_id || modelOptions[0]?.key || '';

  return (
    <div className="app">
      <div className="main-content">
        <Sidebar
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelect={(id) => { selectSession(id); setView('chat'); }}
          onCreate={createSession}
          onDelete={deleteSession}
          onOpenSettings={handleOpenSettings}
          isActive={view === 'settings'}
        />

        {view === 'chat' ? (
          <ChatView
            timeline={timeline}
            error={error}
            onClearError={clearError}
            hasActiveSession={!!activeSession}
            onCreateSession={createSession}
            streaming={isStreaming}
            onCancel={() => useChatStore.getState().cancelRun()}
            input={{ value: inputValue, onChange: setInputValue, onSend: handleSend, disabled: false, sendDisabled: isStreaming, textareaRef, onKeyDown: handleKeyDown }}
            modelOptions={modelOptions}
            selectedModelKey={selectedModelKey}
            onSelectModel={handleSelectModel}
          />
        ) : (
          <SettingsView onBack={handleBackToChat} />
        )}
      </div>
    </div>
  );
}

export default App;