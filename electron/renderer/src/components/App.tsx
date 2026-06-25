import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useChatStore } from '../stores/chat';
import { useSettingsStore } from '../stores/settings';
import { SettingsDialog } from './SettingsDialog';
import { Sidebar } from './Sidebar';
import { ChatView } from './ChatView';
import { buildHistoricalTurns, buildLiveSegments } from '../utils/timeline';
import type { TimelineGroup } from '../types/timeline';

function App() {
  const { t } = useTranslation();
  const {
    sessions, activeSessionId,
    loadSessions, createSession, selectSession, deleteSession,
    error, clearError,
    handleStreamEvent,
  } = useChatStore();

  const { config, loaded, load: loadConfig } = useSettingsStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [inputValue, setInputValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    loadSessions();
    loadConfig();
    window.electronAPI.onStreamEvent(handleStreamEvent);
    return () => { window.electronAPI.onStreamEvent(() => {}); };
  }, []);

  useEffect(() => {
    if (loaded && config && !config.api_key) {
      setSettingsOpen(true);
    }
  }, [loaded, config]);

  // Send
  const handleSend = async () => {
    if (!inputValue.trim()) return;
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
      e.preventDefault();
      handleSend();
    }
  };

  // Derived
  const activeSession = sessions.find((s) => s.id === activeSessionId);
  const currentTurn = activeSession?.currentTurn || [];
  const isStreaming = activeSession?.isStreaming || false;

  const historicalGroups = activeSession ? buildHistoricalTurns(activeSession.messages) : [];
  const live = currentTurn.length > 0 ? buildLiveSegments(currentTurn) : null;
  const timeline: TimelineGroup[] = [
    ...historicalGroups,
    ...(live ? [{ kind: 'liveTurn' as const, id: 'live-turn', segments: live.segments, isStreaming: live.isStreaming }] : []),
  ];

  return (
    <div className="app">
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <div className="main-content">
        <Sidebar
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelect={selectSession}
          onCreate={createSession}
          onDelete={deleteSession}
          onOpenSettings={() => setSettingsOpen(true)}
        />

        <ChatView
          timeline={timeline}
          error={error}
          onClearError={clearError}
          hasActiveSession={!!activeSession}
          onCreateSession={createSession}
          input={{ value: inputValue, onChange: setInputValue, onSend: handleSend, disabled: isStreaming, textareaRef, onKeyDown: handleKeyDown }}
        />
      </div>
    </div>
  );
}

export default App;
