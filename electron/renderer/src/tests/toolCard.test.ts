/**
 * Tests for the short inline summary that follows a tool name in the
 * collapsed tool card header (ToolCard.tsx).
 *
 * The summary's job is to give the user a one-glance scan of what a turn
 * did — it MUST be short (so dozens fit on screen) and MUST surface every
 * command in a chained / piped invocation, not just the first one.
 */
import { describe, it, expect } from 'vitest';
import { getToolSummary, toolResultSucceeded } from '../components/ToolCard';

describe('getToolSummary — file tools (Read / Write / Edit)', () => {
  it('returns the basename of an absolute POSIX path', () => {
    expect(getToolSummary('Read', { path: '/Users/x/code/foo.ts' })).toBe('foo.ts');
    expect(getToolSummary('Write', { path: '/tmp/new_file.py' })).toBe('new_file.py');
    expect(getToolSummary('Edit', { path: '/a/b/c.ts', old_string: 'a', new_string: 'b' })).toBe('c.ts');
  });

  it('handles Windows-style backslash separators', () => {
    expect(getToolSummary('Read', { path: 'C:\\Users\\x\\foo.ts' })).toBe('foo.ts');
  });

  it('returns the full string when there is no separator', () => {
    expect(getToolSummary('Read', { path: 'README.md' })).toBe('README.md');
  });

  it('returns empty string when path is missing or wrong type', () => {
    expect(getToolSummary('Read', {})).toBe('');
    expect(getToolSummary('Read', { path: 42 })).toBe('');
    expect(getToolSummary('Read', { path: '' })).toBe('');
    expect(getToolSummary('Read', null)).toBe('');
  });
});

describe('getToolSummary — Bash', () => {
  it('returns just the command name for a single invocation', () => {
    expect(getToolSummary('Bash', { command: 'cat /etc/passwd' })).toBe('cat');
    expect(getToolSummary('Bash', { command: 'echo hello' })).toBe('echo');
  });

  it('chains multiple commands joined with &&', () => {
    // The case the user actually cares about: surface every command so
    // scanning a long tool list tells you what the agent did.
    expect(getToolSummary('Bash', {
      command: 'cd /Users/typhigh/workspace/clawtao-dev && npx tsc --noEmit',
    })).toBe('cd && npx');

    expect(getToolSummary('Bash', {
      command: 'cd /tmp && grep foo bar.txt && wc -l out',
    })).toBe('cd && grep && wc');
  });

  it('strips leading directory from each command token', () => {
    expect(getToolSummary('Bash', { command: './scripts/build.sh && ls dist' })).toBe('build.sh && ls');
    // `/usr/bin/env` is itself the first token — basename leaves `env`.
    // (Showing `node` would require parser-level awareness of `env` as
    // a shell wrapper, which is out of scope for a one-line summary.)
    expect(getToolSummary('Bash', { command: '/usr/bin/env node app.js' })).toBe('env');
  });

  it('handles leading whitespace in segments', () => {
    expect(getToolSummary('Bash', { command: '   cd /tmp   &&   ls -la' })).toBe('cd && ls');
  });

  it('returns empty string for empty / missing / non-string command', () => {
    expect(getToolSummary('Bash', {})).toBe('');
    expect(getToolSummary('Bash', { command: '' })).toBe('');
    expect(getToolSummary('Bash', { command: 42 })).toBe('');
    expect(getToolSummary('Bash', null)).toBe('');
  });

  it('handles pathological input gracefully (empty segments / only separators)', () => {
    // Multiple adjacent `&&` produce empty segments — they should be
    // filtered out, not crash the renderer.
    expect(getToolSummary('Bash', { command: 'echo a && && echo b' })).toBe('echo && echo');
    expect(getToolSummary('Bash', { command: '   &&  ' })).toBe('');
  });

  it('ignores `2>&1 | tail -N` debug tail — first token wins', () => {
    // The common debugging pattern. We don't strip anything; we just
    // take the first whitespace-delimited token of each &&-segment,
    // so `tail` (and `head`, `grep`, `sed`, …) is never seen.
    expect(getToolSummary('Bash', {
      command: 'cd /Users/typhigh/workspace/clawtao-dev && npx vitest run 2>&1 | tail -8',
    })).toBe('cd && npx');

    expect(getToolSummary('Bash', {
      command: 'cargo build 2>&1 | tail -20',
    })).toBe('cargo');

    expect(getToolSummary('Bash', {
      command: 'make 2>&1 | head -50',
    })).toBe('make');
  });

  it('collapses single-segment commands with pipes to the first token', () => {
    // We deliberately don't parse `|` — pipes inside a single `&&`
    // segment are summarised by the first command only. This is the
    // trade-off for keeping the parser trivial.
    expect(getToolSummary('Bash', { command: 'ls -la | grep foo' })).toBe('ls');
    expect(getToolSummary('Bash', { command: 'cat a.txt; cat b.txt' })).toBe('cat');
    expect(getToolSummary('Bash', { command: 'echo a || echo b' })).toBe('echo');
  });

  it('returns empty when no segment has a parseable first token', () => {
    expect(getToolSummary('Bash', { command: '&&' })).toBe('');
  });
});

describe('getToolSummary — other tools', () => {
  it('returns empty string for tools we do not summarise', () => {
    expect(getToolSummary('WebFetch', { url: 'https://example.com' })).toBe('');
    expect(getToolSummary('WebBrowser', { url: 'https://example.com' })).toBe('');
    expect(getToolSummary('TodoWrite', { items: [] })).toBe('');
    expect(getToolSummary('Grep', { pattern: 'foo', path: '/a/b' })).toBe('');
  });

  it('returns empty string for null / non-object input', () => {
    expect(getToolSummary('Read', null)).toBe('');
    expect(getToolSummary('Read', 'string')).toBe('');
    expect(getToolSummary('Read', 123)).toBe('');
  });
});

describe('toolResultSucceeded — header status indicator', () => {
  it('returns null while the tool is still running (caller shows spinner)', () => {
    expect(toolResultSucceeded(null)).toBeNull();
  });

  it('returns false for the explicit failure prefixes the Rust side emits', () => {
    expect(toolResultSucceeded('Tool error: invalid input: missing path')).toBe(false);
    expect(toolResultSucceeded('Sandbox denied: /etc/passwd is outside the workspace')).toBe(false);
    expect(toolResultSucceeded('Unknown tool: Frobnicate')).toBe(false);
  });

  it('honours the bash `(exit code: N)` trailing marker', () => {
    // Only present when the command produced no stdout/stderr.
    expect(toolResultSucceeded('(exit code: 0)')).toBe(true);
    expect(toolResultSucceeded('(exit code: 1)')).toBe(false);
    expect(toolResultSucceeded('(exit code: 127)')).toBe(false);
    expect(toolResultSucceeded('(exit code: -1)')).toBe(false);
  });

  it('returns true for ordinary tool output (Read/Write/Edit)', () => {
    // Read returns the file content; assume success unless the failure
    // prefix says otherwise.
    expect(toolResultSucceeded('line 1\nline 2\nline 3')).toBe(true);
    expect(toolResultSucceeded('')).toBe(true);
  });

  it('returns true for bash output that has stdout/stderr but no exit-code marker', () => {
    // When bash produces output, format_output() appends stdout/stderr
    // but does NOT append the `(exit code: N)` line — and a non-zero
    // exit without output is the only case where the marker appears.
    // So bare stdout/stderr is presumed success by the marker logic.
    expect(toolResultSucceeded('stdout:\nhello world')).toBe(true);
    expect(toolResultSucceeded('stderr:\noh no')).toBe(true);
  });
});