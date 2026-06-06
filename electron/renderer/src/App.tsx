import { useEffect, useState } from 'react';
import { useChatStore, Message, ToolCall } from './stores/chat';

function formatDate(timestamp: number): string {
  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, {
    month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
}

function App() {
  const {
    sessions, activeSessionId,
    loadSessions, createSession, selectSession,
    streamingText, isStreaming, runningTools,
    error, clearError,
    handleTextDelta, handleChatDone, handleChatStarted,
    handleToolStarted, handleToolResult,
  } = useChatStore();

  const [inputValue, setInputValue] = useState('');

  useEffect(() => {
    loadSessions();
    window.electronAPI.onChatStarted(handleChatStarted as (params: unknown) => void);
    window.electronAPI.onTextDelta(handleTextDelta as (params: unknown) => void);
    window.electronAPI.onChatDone(handleChatDone as (params: unknown) => void);
    window.electronAPI.onToolStarted(handleToolStarted as (params: unknown) => void);
    window.electronAPI.onToolResult(handleToolResult as (params: unknown) => void);
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
          {isStreaming ? 'Thinking...' : 'Ready'}
        </span>
      </header>

      <div className="main-content">
        <aside className="session-list">
          <div className="session-list-header">
            <h2>Sessions</h2>
            <button onClick={createSession} title="New session">+</button>
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
                    ? session.messages[0].content.slice(0, 30) + (session.messages[0].content.length > 30 ? '...' : '')
                    : 'Empty session'}
                </div>
                <div className="session-item-date">{formatDate(session.updated_at)}</div>
              </div>
            ))}
          </div>
        </aside>

        <main className="chat-area">
          {error && <div className="error" onClick={clearError}>{error}</div>}

          {activeSession ? (
            <>
              <div className="messages">
                {activeSession.messages.map((msg) => (
                  <MessageView key={msg.id} message={msg} />
                ))}

                {/* Running tools during streaming */}
                {isStreaming && runningTools.map((tool) => (
                  <ToolCallView
                    key={tool.toolCallId}
                    toolName={tool.toolName}
                    toolInput={tool.toolInput}
                    result={tool.result}
                  />
                ))}

                {/* Streaming text */}
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
                <button type="submit" disabled={!inputValue.trim() || isStreaming}>Send</button>
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
  const roleLabel = message.role === 'user' ? 'You' :
                     message.role === 'tool' ? 'Tool' : 'Assistant';

  if (message.role === 'tool') {
    return (
      <div className="message tool">
        <div className="message-role">Tool result ({message.tool_call_id?.slice(0, 8)}...)</div>
        <div className="message-content">
          <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12 }}>{message.content}</pre>
        </div>
      </div>
    );
  }

  if (message.role === 'assistant' && message.tool_calls) {
    return (
      <div className="message assistant">
        <div className="message-role">Assistant</div>
        <div className="message-content">
          <div className="tool-calls">
            {message.tool_calls.map((tc: ToolCall) => (
              <ToolCallView key={tc.id} toolName={tc.function.name} toolInput={parseArgs(tc.function.arguments)} />
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`message ${message.role}`}>
      <div className="message-role">{roleLabel}</div>
      <div className="message-content">{message.content}</div>
    </div>
  );
}

function ToolCallView({ toolName, toolInput, result }: {
  toolName: string;
  toolInput: unknown;
  result?: string | null;
}) {
  return (
    <div className={`message tool-call ${result ? 'done' : 'pending'}`}>
      <div className="message-role">
        🔧 {toolName} {!result && <span className="tool-spinner">...</span>}
      </div>
      <div className="message-content tool-card">
        <div className="tool-input">
          <strong>Input:</strong> {safeStringify(toolInput)}
        </div>
        {result && (
          <div className="tool-result">
            <strong>Result:</strong>
            <pre style={{ whiteSpace: 'pre-wrap', fontSize: 12, marginTop: 4 }}>{result}</pre>
          </div>
        )}
      </div>
    </div>
  );
}

function parseArgs(args: string): unknown {
  try { return JSON.parse(args); } catch { return args; }
}

function safeStringify(v: unknown): string {
  if (typeof v === 'string') return v;
  try { return JSON.stringify(v); } catch { return String(v); }
}

export default App;
