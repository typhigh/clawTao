// Shared types between Electron and Rust backend

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  timestamp: number;
}

export interface Session {
  id: string;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
}

export interface ChatStreamEvent {
  type: 'text_delta' | 'tool_call' | 'thinking' | 'done' | 'error';
  content?: string;
  toolName?: string;
  toolInput?: unknown;
  runId?: string;
}

export interface ChatSendParams {
  message: string;
  sessionId: string;
}

export interface ChatSendResult {
  runId: string;
}

// JSON-RPC types
export interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: string | number | null;
  method: string;
  params?: Record<string, unknown>;
}

export interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: string | number | null;
  result?: unknown;
  error?: JsonRpcError;
}

export interface JsonRpcNotification {
  jsonrpc: '2.0';
  method: string;
  params?: Record<string, unknown>;
}

export interface JsonRpcError {
  code: number;
  message: string;
  data?: unknown;
}
