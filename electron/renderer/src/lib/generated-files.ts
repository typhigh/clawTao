/**
 * Generated files extraction.
 *
 * Inspects turn-scoped messages + stream events and surfaces files the AI
 * wrote / edited via Write/Edit tool calls. Used by FileChangesPanel to
 * render inline file cards under each turn, and by DiffModal to power the
 * Monaco DiffEditor view.
 *
 * Adapted from clawx's generated-files.ts, slimmed down for clawtao-dev's
 * message model (ToolCall.function.arguments as JSON string, not ContentBlock).
 */
import { diffLines } from 'diff';
import type { Message, StreamEvent } from '../stores/chat';

// ── Types ──────────────────────────────────────────────────────────────

export interface FileEditOp {
  old: string;
  new: string;
}

export interface FileLineStats {
  added: number;
  removed: number;
}

export interface GeneratedFile {
  filePath: string;
  fileName: string;
  ext: string;
  action: 'created' | 'modified';
  /** Full new content of the file (from Write tool `content` input). */
  fullContent?: string;
  /** Ordered edits applied during the turn (from Edit tool old/new strings). */
  edits?: FileEditOp[];
  /** Pre-write file content — captured by the backend before the tool runs. */
  baseline?: string;
  /** Index of the latest message/event that touched this file (stable sort). */
  lastSeenIndex: number;
}

// ── Tool name sets ─────────────────────────────────────────────────────

const WRITE_TOOLS = new Set([
  'Write',
  'write_file',
  'create_file',
  'WriteFile',
  'createFile',
  'write',
]);

const EDIT_TOOLS = new Set([
  'Edit',
  'edit',
  'edit_file',
  'EditFile',
  'StrReplace',
  'str_replace',
]);

// ── Helpers ────────────────────────────────────────────────────────────

function basenameOf(path: string): string {
  if (!path) return '';
  const norm = path.replace(/\\/g, '/');
  const last = norm.lastIndexOf('/');
  return last >= 0 ? norm.slice(last + 1) : norm;
}

function extnameOf(path: string): string {
  const name = basenameOf(path);
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return '';
  return name.slice(dot);
}

function normaliseEol(text: string): string {
  return text.replace(/\r\n/g, '\n');
}

function countLogicalLines(text: string): number {
  const normalized = normaliseEol(text);
  if (!normalized) return 0;
  const parts = normalized.split('\n');
  return normalized.endsWith('\n') ? Math.max(1, parts.length - 1) : parts.length;
}

// ── Payload detection ──────────────────────────────────────────────────

/** True when enough tool payload was captured to render a diff. */
export function generatedFileHasDiffPayload(
  file: Pick<GeneratedFile, 'fullContent' | 'edits'>,
): boolean {
  if (file.fullContent != null) return true;
  if (file.edits?.length) {
    return file.edits.some((op) => (op.old ?? '') !== '' || (op.new ?? '') !== '');
  }
  return false;
}

// ── Line stats ─────────────────────────────────────────────────────────

const SNIPPET_SEPARATOR = '\n\n';

function joinEditText(edits: FileEditOp[], side: 'old' | 'new'): string {
  return edits.map((op) => normaliseEol(op[side] ?? '')).join(SNIPPET_SEPARATOR);
}

export function computeLineStats(file: GeneratedFile): FileLineStats | null {
  // Edit tools: diff the old vs new strings
  if (file.edits?.length) {
    return diffLineStats(joinEditText(file.edits, 'old'), joinEditText(file.edits, 'new'));
  }

  if (file.fullContent == null) return null;

  // Write tool with baseline: true before/after diff
  if (file.baseline != null) {
    return diffLineStats(file.baseline, file.fullContent);
  }

  // Write tool without baseline: all lines are added
  return { added: countLogicalLines(file.fullContent), removed: 0 };
}

function diffLineStats(oldText: string, newText: string): FileLineStats {
  const pieces = diffLines(normaliseEol(oldText), normaliseEol(newText));
  let added = 0;
  let removed = 0;
  for (const piece of pieces) {
    const count =
      typeof piece.count === 'number'
        ? piece.count
        : countLogicalLines(piece.value);
    if (piece.added) added += count;
    if (piece.removed) removed += count;
  }
  return { added, removed };
}

// ── Diff pair for Monaco ───────────────────────────────────────────────

export interface DiffPair {
  original: string;
  modified: string;
}

/** Build the left/right pair for Monaco DiffEditor. */
export function computeDiffPair(file: GeneratedFile): DiffPair {
  // Edit tools: left = joined old_strings, right = joined new_strings
  if (file.edits?.length) {
    return {
      original: joinEditText(file.edits, 'old'),
      modified: joinEditText(file.edits, 'new'),
    };
  }

  // Write tool with baseline: true before/after
  if (file.baseline != null) {
    return { original: file.baseline, modified: file.fullContent ?? '' };
  }

  // Write tool without baseline: left empty, right = full content
  return { original: '', modified: file.fullContent ?? '' };
}

// ── Arg parsing ────────────────────────────────────────────────────────

/** Parse tool call arguments: ToolCall.function.arguments is a JSON string. */
function parseArgs(tc: { function: { arguments: string } }): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(tc.function.arguments);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
    return null;
  } catch {
    return null;
  }
}

function pickFilePath(args: Record<string, unknown>): string | null {
  for (const key of ['file_path', 'filepath', 'path', 'fileName', 'file_name', 'target_path']) {
    const v = args[key];
    if (typeof v === 'string' && v.trim()) return v.trim();
  }
  return null;
}

// ── Extract from messages ──────────────────────────────────────────────

/**
 * Walk messages in [triggerIndex, segmentEnd] (inclusive) and collect
 * unique files written or edited by tool calls in that window.
 * Deduplicates by filePath; merge strategy keeps the latest fullContent
 * and accumulates edits.
 */
export function extractGeneratedFiles(
  messages: Message[],
  triggerIndex: number,
  segmentEnd: number,
): GeneratedFile[] {
  const map = new Map<string, GeneratedFile>();
  const start = Math.max(0, Math.min(triggerIndex + 1, messages.length));
  const end = Math.max(start - 1, Math.min(segmentEnd, messages.length - 1));

  for (let i = start; i <= end; i += 1) {
    const message = messages[i];
    if (!message || message.role !== 'assistant') continue;
    if (!message.tool_calls?.length) continue;

    for (const tc of message.tool_calls) {
      const name = tc.function.name;
      if (!name) continue;

      const args = parseArgs(tc);
      if (!args) continue;

      const filePath = pickFilePath(args);
      if (!filePath) continue;

      const isWrite = WRITE_TOOLS.has(name);
      const isEdit = EDIT_TOOLS.has(name);

      if (!isWrite && !isEdit) continue;

      const existing = map.get(filePath);

      if (isWrite) {
        const content = pickWriteContent(args);
        // Baseline may come from the matching tool_result StreamEvent
        // (stored later via enrichWithStreamEvents).
        map.set(filePath, {
          filePath,
          fileName: basenameOf(filePath),
          ext: extnameOf(filePath),
          action: existing?.action === 'created' ? 'created' : 'modified',
          fullContent: content,
          edits: undefined,
          baseline: existing?.baseline,
          lastSeenIndex: i,
        });
        continue;
      }

      if (isEdit) {
        const newOps = pickEditOps(args);
        const merged: GeneratedFile = {
          filePath,
          fileName: basenameOf(filePath),
          ext: extnameOf(filePath),
          action: 'modified',
          fullContent: existing?.fullContent,
          edits: [...(existing?.edits ?? []), ...newOps],
          baseline: existing?.baseline,
          lastSeenIndex: i,
        };
        map.set(filePath, merged);
        continue;
      }
    }
  }

  return Array.from(map.values()).sort((a, b) => a.lastSeenIndex - b.lastSeenIndex);
}

// ── Enrich with stream events (live turn + baseline) ───────────────────

/**
 * Merge fileChange info from stream events into the generated file list.
 * Stream events come from the live turn (currentTurn in chat store) and
 * carry the backend-captured baseline (oldContent) from tool_result notifications.
 */
export function enrichWithStreamEvents(
  files: GeneratedFile[],
  streamEvents: StreamEvent[],
): GeneratedFile[] {
  const result = new Map<string, GeneratedFile>();
  for (const f of files) result.set(f.filePath, { ...f });

  for (const ev of streamEvents) {
    // tool_call events: extract write/edit files from live turn
    if (ev.kind === 'tool_call' && ev.toolName && ev.input) {
      const input = ev.input as Record<string, unknown> | null;
      if (!input) continue;
      const filePath = pickFilePath(input);
      if (!filePath) continue;
      const isWrite = WRITE_TOOLS.has(ev.toolName);
      const isEdit = EDIT_TOOLS.has(ev.toolName);
      if (!isWrite && !isEdit) continue;

      const existing = result.get(filePath);
      if (isWrite) {
        const content = pickWriteContent(input);
        result.set(filePath, {
          filePath,
          fileName: basenameOf(filePath),
          ext: extnameOf(filePath),
          action: existing?.action === 'created' ? 'created' : 'modified',
          fullContent: content,
          edits: undefined,
          baseline: existing?.baseline,
          lastSeenIndex: 0,
        });
      } else if (isEdit) {
        const newOps = pickEditOps(input);
        result.set(filePath, {
          filePath,
          fileName: basenameOf(filePath),
          ext: extnameOf(filePath),
          action: 'modified',
          fullContent: existing?.fullContent,
          edits: [...(existing?.edits ?? []), ...newOps],
          baseline: existing?.baseline,
          lastSeenIndex: 0,
        });
      }
    }

    // tool_result with fileChange: apply baseline
    if (ev.kind === 'tool_result' && ev.fileChange) {
      const fc = ev.fileChange;
      // Find matching file by path
      for (const [key, f] of result) {
        if (f.filePath === fc.path || key === fc.path) {
          result.set(key, {
            ...f,
            action: fc.action,
            baseline: fc.oldContent ?? undefined,
            fullContent: f.fullContent ?? fc.newContent,
          });
          break;
        }
      }
      // If no existing entry for this path, create one from fileChange alone
      const alreadyExists = Array.from(result.values()).some(
        (f) => f.filePath === fc.path,
      );
      if (!alreadyExists) {
        result.set(fc.path, {
          filePath: fc.path,
          fileName: basenameOf(fc.path),
          ext: extnameOf(fc.path),
          action: fc.action,
          fullContent: fc.newContent,
          baseline: fc.oldContent ?? undefined,
          lastSeenIndex: 0,
        });
      }
    }
  }

  return Array.from(result.values()).sort((a, b) => a.lastSeenIndex - b.lastSeenIndex);
}

/** Extract generated files purely from stream events (live turn). */
export function extractFromStreamEvents(
  streamEvents: StreamEvent[],
): GeneratedFile[] {
  return enrichWithStreamEvents([], streamEvents);
}

// ── Content extraction helpers ─────────────────────────────────────────

function pickWriteContent(args: Record<string, unknown>): string | undefined {
  for (const key of [
    'content', 'contents', 'text', 'body', 'data',
    'new_content', 'new_string', 'newString', 'string', 'source',
  ]) {
    const v = args[key];
    if (typeof v === 'string') return v;
  }
  return undefined;
}

const OLD_KEYS = [
  'old_string', 'oldString', 'old_str', 'oldStr',
  'old_text', 'oldText', 'old', 'oldContent', 'before', 'find', 'search',
];
const NEW_KEYS = [
  'new_string', 'newString', 'new_str', 'newStr',
  'new_text', 'newText', 'new', 'newContent', 'after', 'replace', 'replacement',
];

function pickStringByKeys(
  rec: Record<string, unknown>,
  keys: string[],
): string | undefined {
  for (const k of keys) {
    const v = rec[k];
    if (typeof v === 'string') return v;
  }
  return undefined;
}

function pickEditOps(input: Record<string, unknown>): FileEditOp[] {
  const ops: FileEditOp[] = [];
  const singleOld = pickStringByKeys(input, OLD_KEYS);
  const singleNew = pickStringByKeys(input, NEW_KEYS);
  if (singleOld !== undefined || singleNew !== undefined) {
    ops.push({ old: singleOld ?? '', new: singleNew ?? '' });
  }
  const edits = input.edits;
  if (Array.isArray(edits)) {
    for (const edit of edits as Array<Record<string, unknown>>) {
      const o = pickStringByKeys(edit, OLD_KEYS) ?? '';
      const n = pickStringByKeys(edit, NEW_KEYS) ?? '';
      if (o !== '' || n !== '') ops.push({ old: o, new: n });
    }
  }
  return ops;
}
