import type { ImageAttachment, StreamEvent } from '../stores/chat';

export type AssistantSegment =
  | { kind: 'text'; id: string; content: string; timestamp: number }
  | { kind: 'tool'; id: string; toolName: string; toolInput: unknown; timestamp: number }
  | { kind: 'toolResult'; id: string; content: string; toolCallId?: string; timestamp: number }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean }
  | { kind: 'thinking'; id: string; content: string; timestamp: number };

export type TurnSegment =
  | { kind: 'text'; content: string }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean }
  | { kind: 'thinking'; content: string }
  | { kind: 'todo'; todos: { step: string; status: string }[] }
  | { kind: 'compacted'; messageCount?: number; warning?: string };

export type TimelineGroup =
  | { kind: 'user'; id: string; content: string; timestamp: number; images?: ImageAttachment[] }
  | { kind: 'agentTurn'; id: string; segments: AssistantSegment[]; conclusion: string | null; isStreaming: boolean }
  | { kind: 'liveTurn'; id: string; segments: TurnSegment[]; isStreaming: boolean };
