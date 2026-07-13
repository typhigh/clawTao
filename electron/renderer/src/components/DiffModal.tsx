import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { DiffEditor, loader, type Monaco } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';
import { useWorkspaceChangesStore } from '../stores/workspace-changes';
import { computeDiffPair, computeLineStats } from '../lib/generated-files';
import { FileChangeCard } from './FileChangeCard';

// Configure @monaco-editor/react to use the locally bundled Monaco
// instead of downloading from CDN.
loader.config({ monaco });

/** Map a file extension to a Monaco language identifier. */
function languageForPath(filePath: string): string {
  const ext = filePath.slice(filePath.lastIndexOf('.')).toLowerCase();
  const map: Record<string, string> = {
    '.ts': 'typescript', '.tsx': 'typescript',
    '.js': 'javascript', '.jsx': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript',
    '.py': 'python',
    '.rb': 'ruby',
    '.go': 'go',
    '.rs': 'rust',
    '.java': 'java',
    '.kt': 'kotlin',
    '.swift': 'swift',
    '.c': 'c', '.cc': 'cpp', '.cpp': 'cpp', '.h': 'c', '.hpp': 'cpp', '.cs': 'csharp',
    '.json': 'json', '.yaml': 'yaml', '.yml': 'yaml', '.toml': 'toml', '.xml': 'xml',
    '.sh': 'shell', '.bash': 'shell', '.zsh': 'shell', '.ps1': 'powershell',
    '.html': 'html', '.htm': 'html', '.css': 'css', '.scss': 'scss', '.sass': 'sass', '.less': 'less',
    '.sql': 'sql', '.lua': 'lua', '.r': 'r', '.dart': 'dart',
    '.md': 'markdown', '.markdown': 'markdown',
    '.txt': 'plaintext',
  };
  return map[ext] ?? 'plaintext';
}

export function DiffModal() {
  const { t } = useTranslation();
  const {
    modalOpen, focusedFile, allFiles, focusedFileIndex,
    closeDiff, nextFile, prevFile,
  } = useWorkspaceChangesStore();

  // Close on Escape
  useEffect(() => {
    if (!modalOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') closeDiff();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [modalOpen, closeDiff]);

  if (!modalOpen || !focusedFile) return null;

  const diffPair = computeDiffPair(focusedFile);
  const stats = computeLineStats(focusedFile);
  const language = languageForPath(focusedFile.filePath);
  const hasNav = allFiles.length > 1;

  return (
    <div className="diff-modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) closeDiff(); }}>
      <div className="diff-modal">
        {/* Header */}
        <div className="diff-modal-header">
          <span className="diff-modal-title">
            {focusedFile.fileName}
          </span>
          <div className="diff-modal-meta">
            <span className={`file-change-badge ${focusedFile.action === 'created' ? 'file-change-badge--created' : 'file-change-badge--modified'}`}>
              {focusedFile.action === 'created' ? t('workspaceChanges.created') : t('workspaceChanges.modified')}
            </span>
            {stats && (
              <span className="file-change-card__stats" style={{ fontSize: 12 }}>
                {stats.added > 0 && <span className="file-change-card__added">+{stats.added}</span>}
                {stats.removed > 0 && <span className="file-change-card__removed">-{stats.removed}</span>}
              </span>
            )}
            {hasNav && (
              <>
                <button className="diff-modal-btn" onClick={prevFile} title={t('workspaceChanges.prev')}>
                  ‹ {t('workspaceChanges.prev')}
                </button>
                <span style={{ fontSize: 11, color: '#888' }}>
                  {focusedFileIndex + 1}/{allFiles.length}
                </span>
                <button className="diff-modal-btn" onClick={nextFile} title={t('workspaceChanges.next')}>
                  {t('workspaceChanges.next')} ›
                </button>
              </>
            )}
          </div>
          <button className="diff-modal-close" onClick={closeDiff} aria-label={t('workspaceChanges.close')}>✕</button>
        </div>

        {/* Body — Monaco DiffEditor */}
        <div className="diff-modal-body">
          <DiffEditor
            original={diffPair.original}
            modified={diffPair.modified}
            language={language}
            options={{
              readOnly: true,
              renderSideBySide: true,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              wordWrap: 'on',
              automaticLayout: true,
              lineNumbers: 'on',
              folding: true,
              renderOverviewRuler: false,
            }}
            loading={<div style={{ padding: 24, color: '#888', fontSize: 13 }}>{t('common.loading')}</div>}
          />
        </div>
      </div>
    </div>
  );
}
