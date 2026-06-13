import { useEffect, useState } from 'react';
import { useChatStore, Message, ToolCall } from './stores/chat';
import { useSettingsStore } from './stores/settings';
import { SettingsDialog } from './components/SettingsDialog';

function formatRelative(timestamp: number): string {
  const diffMs = Date.now() - timestamp;
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return '刚刚';
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}分钟前`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}小时前`;
  const day = Math.floor(hr / 24);
  if (day < 30) return `${day}天前`;
  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function App() {
  const {
    sessions, activeSessionId,
    loadSessions, createSession, selectSession, deleteSession,
    streamingText, isStreaming, runningTools,
    error, clearError,
    handleTextDelta, handleChatDone, handleChatStarted,
    handleToolStarted, handleToolResult,
  } = useChatStore();

  const { config, loaded, load: loadConfig } = useSettingsStore();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [inputValue, setInputValue] = useState('');

  useEffect(() => {
    loadSessions();
    loadConfig();
    window.electronAPI.onChatStarted(handleChatStarted as (params: unknown) => void);
    window.electronAPI.onTextDelta(handleTextDelta as (params: unknown) => void);
    window.electronAPI.onChatDone(handleChatDone as (params: unknown) => void);
    window.electronAPI.onToolStarted(handleToolStarted as (params: unknown) => void);
    window.electronAPI.onToolResult(handleToolResult as (params: unknown) => void);
  }, []);

  // Auto-open settings if no API key configured
  useEffect(() => {
    if (loaded && config && !config.api_key) {
      setSettingsOpen(true);
    }
  }, [loaded, config]);

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
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />

      <div className="main-content">
        <aside className="sidebar">
          {/* ── Config header ── */}
          <div className="sidebar-section config-section">
            <div className="sidebar-section-header">
              <h2>⚙️ Configuration</h2>
              <button className="btn-icon" onClick={() => setSettingsOpen(true)} title="Settings">
                ⚙️
              </button>
            </div>
          </div>

          {/* ── Spacer: pushes sessions to the middle ── */}
          <div className="sidebar-spacer" />

          {/* ── Sessions section ── */}
          <div className="sidebar-section sessions-section">
            <div className="sidebar-section-header">
              <h2>Sessions</h2>
              <button onClick={createSession} title="New session">+</button>
            </div>
            <div className="session-items">
              {sessions.length === 0 ? (
                <div className="session-empty">No sessions yet</div>
              ) : (
                sessions.map((session) => {
                  const isActive = session.id === activeSessionId;
                  const isRunning = isActive && isStreaming;
                  return (
                    <div
                      key={session.id}
                      className={`session-item ${isActive ? 'active' : ''}`}
                      onClick={() => selectSession(session.id)}
                    >
                      <span className={`session-item-status ${isRunning ? 'running' : 'done'}`}>
                        {isRunning ? <SpinnerIcon /> : <CheckIcon />}
                      </span>
                      <div className="session-item-title">{(session as any).title || 'Empty session'}</div>
                      <div className="session-item-time">{formatRelative(session.updated_at)}</div>
                      <button
                        className="session-delete-btn"
                        onClick={(e) => { e.stopPropagation(); deleteSession(session.id); }}
                        title="Delete session"
                      >×</button>
                    </div>
                  );
                })
              )}
            </div>
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

function CheckIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M3 8.5L6.5 12L13 4.5" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function SpinnerIcon() {
  return (
    <svg className="session-spinner" width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeDasharray="28" strokeDashoffset="20" />
    </svg>
  );
}

function safeStringify(v: unknown): string {
  if (typeof v === 'string') return v;
  try { return JSON.stringify(v); } catch { return String(v); }
}

export default App;
