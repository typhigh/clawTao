import { useTranslation } from 'react-i18next';
import { fileIconFor } from './FileIcon';
import { computeLineStats, type GeneratedFile } from '../lib/generated-files';

interface Props {
  file: GeneratedFile;
  onClick: () => void;
}

/** Single clickable card showing file change summary: icon, name, path, +N/-N, badge. */
export function FileChangeCard({ file, onClick }: Props) {
  const { t } = useTranslation();
  const stats = computeLineStats(file);
  const icon = fileIconFor(file.ext);

  return (
    <button type="button" className="file-change-card" onClick={onClick} title={file.filePath}>
      <span className="file-change-card__icon">{icon}</span>
      <span className="file-change-card__name">{file.fileName}</span>
      <span className="file-change-card__path">{file.filePath}</span>
      {stats && (
        <span className="file-change-card__stats">
          {stats.added > 0 && <span className="file-change-card__added">+{stats.added}</span>}
          {stats.removed > 0 && <span className="file-change-card__removed">-{stats.removed}</span>}
        </span>
      )}
      <span className={`file-change-badge ${file.action === 'created' ? 'file-change-badge--created' : 'file-change-badge--modified'}`}>
        {file.action === 'created' ? t('workspaceChanges.created') : t('workspaceChanges.modified')}
      </span>
    </button>
  );
}
