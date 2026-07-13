import { useEffect, useState } from 'react';
import { useChatStore } from '../stores/chat';
import { useSettingsStore } from '../stores/settings';
import { SettingsView } from './SettingsView';
import { Sidebar } from './Sidebar';
import { ChatScreen } from './ChatScreen';
import { DiffModal } from './DiffModal';

type View = 'chat' | 'settings';

function App() {
  const {
    sessions, activeSessionId,
    loadSessions, createSession, selectSession, deleteSession,
    handleStreamEvent,
  } = useChatStore();
  const { config, loaded, load: loadConfig } = useSettingsStore();
  const [view, setView] = useState<View>('chat');

  useEffect(() => {
    loadSessions();
    loadConfig();
    window.electronAPI.onStreamEvent(handleStreamEvent);
    return () => { window.electronAPI.onStreamEvent(() => {}); };
  }, []);

  // First-run with no api_key anywhere → land in settings.
  useEffect(() => {
    if (loaded && config && (config.llm.providers.length === 0 || config.llm.providers.every(p => !p.api_key))) {
      setView('settings');
    }
  }, [loaded, config]);

  const handleOpenSettings = () => {
    useChatStore.setState({ activeSessionId: null });
    setView('settings');
  };
  const handleBackToChat = () => setView('chat');

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

        {view === 'chat' ? <ChatScreen /> : <SettingsView onBack={handleBackToChat} />}
      </div>
      <DiffModal />
    </div>
  );
}

export default App;