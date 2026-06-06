import { useEffect, useState } from 'react';
import { useChatStore, Message } from './stores/chat';

function formatDate(timestamp: number): string {
  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function App() {
  const {
    sessions,
    activeSessionId,
    loadSessions,
    createSession,
    selectSession,
    streamingText,
    isStreaming,
    error,
    clearError,
    handleTextDelta,
    handleChatDone,
    handleChatStarted,
  } = useChatStore();

  const [inputValue, setInputValue] = useState('');

  // Load sessions on mount
  useEffect(() => {
    loadSessions();

    // Set up event listeners
    window.electronAPI.onChatStarted(handleChatStarted as (params: unknown) => void);
    window.electronAPI.onTextDelta(handleTextDelta as (params: unknown) => void);
    window.electronAPI.onChatDone(handleChatDone as (params: unknown) => void);
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim() || isStreaming) return;

    const text = inputValue;
    setInputValue('');
    await useChatStore.getState().sendMessage(text);
  };

  const activeSession = sessions.find((s) => s.id === activeSessionId);

  return (
    <div className="app">
      <header className="header">
        <h1>ClawTao</h1>
        <span style={{ fontSize: 12, opacity: 0.7 }}>
          {isStreaming ? 'Streaming...' : 'Ready'}
        </span>
      </header>

      <div className="main-content">
        <aside className="session-list">
          <div className="session-list-header">
            <h2>Sessions</h2>
            <button onClick={createSession} title="New session">
              +
            </button>
          </div>
          <div className="session-items">
            {sessions.map((session) => (
              <div
                key={session.id}
                className={`session-item ${session.id === activeSessionId ? 'active' : ''}`}
                onClick={() => selectSession(session.id)}
              >
                <div className="session-item-title">
                  {session.messages.length > 0
                    ? session.messages[0].content.slice(0, 30) +
                      (session.messages[0].content.length > 30 ? '...' : '')
                    : 'Empty session'}
                </div>
                <div className="session-item-date">
                  {formatDate(session.updated_at)}
                </div>
              </div>
            ))}
          </div>
        </aside>

        <main className="chat-area">
          {error && (
            <div className="error" onClick={clearError}>
              {error}
            </div>
          )}

          {activeSession ? (
            <>
              <div className="messages">
                {activeSession.messages.map((msg) => (
                  <MessageView key={msg.id} message={msg} />
                ))}

                {isStreaming && streamingText && (
                  <div className="message assistant streaming">
                    <div className="message-role">Assistant</div>
                    <div className="message-content">{streamingText}</div>
                  </div>
                )}
              </div>

              <form className="input-area" onSubmit={handleSubmit}>
                <input
                  type="text"
                  placeholder="Type your message..."
                  value={inputValue}
                  onChange={(e) => setInputValue(e.target.value)}
                  disabled={isStreaming}
                />
                <button type="submit" disabled={!inputValue.trim() || isStreaming}>
                  Send
                </button>
              </form>
            </>
          ) : (
            <div className="empty-state">
              <p>No session selected</p>
              <button onClick={createSession}>Create a session</button>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

function MessageView({ message }: { message: Message }) {
  return (
    <div className={`message ${message.role}`}>
      <div className="message-role">
        {message.role === 'user' ? 'You' : 'Assistant'}
      </div>
      <div className="message-content">{message.content}</div>
    </div>
  );
}

export default App;
