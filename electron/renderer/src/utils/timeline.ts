import type { Message, StreamEvent } from '../stores/chat';
import type { AssistantSegment, TimelineGroup, TurnSegment } from '../types/timeline';

// ── buildHistoricalTurns ────────────────────────────────────────────

export function buildHistoricalTurns(messages: Message[]): TimelineGroup[] {
  const groups: TimelineGroup[] = [];
  let currentTurn: AssistantSegment[] | null = null;
  let currentConclusion: string | null = null;
  let turnIdCounter = 0;

  const flushTurn = () => {
    if ((currentTurn && currentTurn.length > 0) || currentConclusion !== null) {
      groups.push({
        kind: 'agentTurn',
        id: `turn-${turnIdCounter++}`,
        segments: currentTurn ?? [],
        conclusion: currentConclusion,
        isStreaming: false,
      });
    }
    currentTurn = null;
    currentConclusion = null;
  };

  for (const msg of messages) {
    if (msg.role === 'user') {
      flushTurn();
      groups.push({ kind: 'user', id: msg.id, content: msg.content, timestamp: msg.timestamp, images: msg.images });
    } else if (msg.role === 'assistant') {
      if (msg.tool_calls && msg.tool_calls.length > 0) {
        if (!currentTurn) currentTurn = [];
        if (msg.thinking) {
          currentTurn.push({
            kind: 'thinking', id: `${msg.id}-thinking`, content: msg.thinking, timestamp: msg.timestamp,
          });
        }
        if (msg.content) {
          currentTurn.push({
            kind: 'text', id: `${msg.id}-text`, content: msg.content, timestamp: msg.timestamp,
          });
        }
        for (const tc of msg.tool_calls) {
          let parsedArgs: unknown = tc.function.arguments;
          try { parsedArgs = JSON.parse(tc.function.arguments); } catch { /* keep as string */ }
          currentTurn.push({
            kind: 'tool', id: tc.id, toolName: tc.function.name, toolInput: parsedArgs, timestamp: msg.timestamp,
          });
        }
      } else if (msg.content || msg.thinking) {
        if (msg.thinking) {
          if (!currentTurn) currentTurn = [];
          currentTurn.push({
            kind: 'thinking', id: `${msg.id}-thinking`, content: msg.thinking, timestamp: msg.timestamp,
          });
        }
        if (msg.content) {
          currentConclusion = msg.content;
        }
      }
    } else if (msg.role === 'tool') {
      if (!currentTurn) currentTurn = [];
      currentTurn.push({
        kind: 'toolResult', id: msg.id, content: msg.content, toolCallId: msg.tool_call_id, timestamp: msg.timestamp,
      });
    }
  }
  flushTurn();
  return groups;
}

// ── buildLiveSegments ───────────────────────────────────────────────

export function buildLiveSegments(events: StreamEvent[]): { segments: TurnSegment[]; isStreaming: boolean } {
  const hasDone = events.some((e) => e.kind === 'done');
  const segments: TurnSegment[] = [];
  let textBuf = '';
  let thinkingBuf = '';
  let pendingTool: { id: string; name: string; input: unknown } | null = null;

  const flushThinking = () => {
    if (thinkingBuf) { segments.push({ kind: 'thinking', content: thinkingBuf }); thinkingBuf = ''; }
  };
  const flushText = () => {
    if (textBuf) { segments.push({ kind: 'text', content: textBuf }); textBuf = ''; }
  };
  const flushPending = () => {
    if (pendingTool) {
      segments.push({ kind: 'toolPair', id: pendingTool.id, toolName: pendingTool.name, toolInput: pendingTool.input, result: null, pending: true });
      pendingTool = null;
    }
  };

  for (const ev of events) {
    switch (ev.kind) {
      case 'started': case 'done': break;
      case 'todo':
        // Replace the previous todo segment — only latest is shown.
        for (let i = segments.length - 1; i >= 0; i--) {
          if (segments[i].kind === 'todo') { segments.splice(i, 1); break; }
        }
        if (ev.todos && ev.todos.length > 0) {
          segments.push({ kind: 'todo', todos: ev.todos });
        }
        break;
      case 'thinking': thinkingBuf += ev.delta!; break;
      case 'text': flushThinking(); textBuf += ev.delta!; break;
      case 'tool_call':
        flushThinking(); flushText(); flushPending();
        pendingTool = { id: ev.toolCallId!, name: ev.toolName!, input: ev.input };
        break;
      case 'tool_result':
        flushText();
        if (pendingTool && pendingTool.id === ev.toolCallId) {
          segments.push({ kind: 'toolPair', id: pendingTool.id, toolName: pendingTool.name, toolInput: pendingTool.input, result: ev.output!, pending: false });
          pendingTool = null;
        } else {
          segments.push({ kind: 'toolPair', id: ev.toolCallId!, toolName: ev.toolName!, toolInput: null, result: ev.output!, pending: false });
        }
        break;
      case 'compacting':
        flushThinking(); flushText(); flushPending();
        break;
      case 'compacted':
        flushThinking(); flushText(); flushPending();
        segments.push({ kind: 'compacted', messageCount: ev.messageCount, warning: ev.warning });
        break;
    }
  }
  flushThinking(); flushText(); flushPending();
  return { segments, isStreaming: !hasDone };
}

// ── pairToolWithResults ─────────────────────────────────────────────

export function pairToolWithResults(segments: AssistantSegment[]): AssistantSegment[] {
  const out: AssistantSegment[] = [];
  const pending = new Map<string, Extract<AssistantSegment, { kind: 'tool' }>>();
  for (const s of segments) {
    if (s.kind === 'tool') {
      pending.set(s.id, s);
    } else if (s.kind === 'toolResult') {
      const matched = s.toolCallId ? pending.get(s.toolCallId) : undefined;
      if (matched) {
        pending.delete(s.toolCallId!);
        out.push({ kind: 'toolPair', id: matched.id, toolName: matched.toolName, toolInput: matched.toolInput, result: s.content, pending: false });
      } else {
        out.push(s);
      }
    } else {
      out.push(s);
    }
  }
  for (const tool of pending.values()) {
    out.push({ kind: 'toolPair', id: tool.id, toolName: tool.toolName, toolInput: tool.toolInput, result: null, pending: true });
  }
  return out;
}

// ── countTurnSegments ───────────────────────────────────────────────

export function countTurnSegments(segments: AssistantSegment[]): { toolCount: number; processCount: number } {
  let toolCount = 0, processCount = 0;
  for (const s of segments) {
    if (s.kind === 'tool' || s.kind === 'toolPair') toolCount++;
    else if (s.kind === 'text') processCount++;
  }
  return { toolCount, processCount };
}
