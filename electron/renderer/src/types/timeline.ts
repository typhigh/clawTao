import type { StreamEvent } from '../stores/chat';

export type AssistantSegment =
  | { kind: 'text'; id: string; content: string; timestamp: number }
  | { kind: 'tool'; id: string; toolName: string; toolInput: unknown; timestamp: number }
  | { kind: 'toolResult'; id: string; content: string; toolCallId?: string; timestamp: number }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean }
  | { kind: 'thinking'; id: string; content: string; timestamp: number };

export type TurnSegment =
  | { kind: 'text'; content: string }
  | { kind: 'toolPair'; id: string; toolName: string; toolInput: unknown; result: string | null; pending: boolean }
  | { kind: 'thinking'; content: string };

export type TimelineGroup =
  | { kind: 'user'; id: string; content: string; timestamp: number }
  | { kind: 'agentTurn'; id: string; segments: AssistantSegment[]; conclusion: string | null; isStreaming: boolean }
  | { kind: 'liveTurn'; id: string; segments: TurnSegment[]; isStreaming: boolean };
