import { useTranslation } from 'react-i18next';
import { FileChangeCard } from './FileChangeCard';
import type { GeneratedFile } from '../lib/generated-files';

interface Props {
  files: GeneratedFile[];
  onFileClick: (file: GeneratedFile) => void;
}

/** Horizontal row of file change cards under each agent turn. Hides when empty. */
export function FileChangesPanel({ files, onFileClick }: Props) {
  const { t } = useTranslation();

  if (!files || files.length === 0) return null;

  return (
    <div className="file-changes-panel">
      <div className="file-changes-title">
        {t('workspaceChanges.title', { count: files.length })}
      </div>
      <div className="file-changes-cards">
        {files.map((f) => (
          <FileChangeCard key={f.filePath} file={f} onClick={() => onFileClick(f)} />
        ))}
      </div>
    </div>
  );
}
