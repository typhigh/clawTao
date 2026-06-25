import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { LanguageSwitcher } from './LanguageSwitcher';
import { SpinnerIcon, CheckIcon } from './icons';
import { formatRelative } from '../utils/format';
import type { Session } from '../stores/chat';

interface Props {
  sessions: Session[];
  activeSessionId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
  onOpenSettings: () => void;
}

export function Sidebar({ sessions, activeSessionId, onSelect, onCreate, onDelete, onOpenSettings }: Props) {
  const { t } = useTranslation();

  return (
    <aside className="sidebar">
      <div className="sidebar-section config-section">
        <div className="sidebar-section-header">
          <h2>{t('sidebar.configuration')}</h2>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <LanguageSwitcher />
            <button className="btn-icon" onClick={onOpenSettings} title={t('sidebar.configuration')}>⚙️</button>
          </div>
        </div>
      </div>

      <div className="sidebar-spacer" />

      <div className="sidebar-section sessions-section">
        <div className="sidebar-section-header">
          <h2>{t('sidebar.sessions')}</h2>
          <button onClick={onCreate} title={t('sidebar.newSession')}>+</button>
        </div>
        <div className="session-items">
          {sessions.length === 0 ? (
            <div className="session-empty">{t('sidebar.noSessions')}</div>
          ) : (
            sessions.map((session) => (
              <SessionItem
                key={session.id}
                session={session}
                isActive={session.id === activeSessionId}
                onSelect={onSelect}
                onDelete={onDelete}
                t={t}
              />
            ))
          )}
        </div>
      </div>
    </aside>
  );
}

function SessionItem({ session, isActive, onSelect, onDelete, t }: {
  session: Session; isActive: boolean; onSelect: (id: string) => void; onDelete: (id: string) => void; t: TFunction;
}) {
  const isRunning = session.isStreaming || false;

  return (
    <div
      className={`session-item ${isActive ? 'active' : ''}`}
      onClick={() => onSelect(session.id)}
    >
      <span className={`session-item-status ${isRunning ? 'running' : 'done'}`}>
        {isRunning ? <SpinnerIcon /> : <CheckIcon />}
      </span>
      <div className="session-item-title">{session.title || t('sidebar.emptySession')}</div>
      <div className="session-item-time">{formatRelative(t, session.updated_at)}</div>
      <button
        className="session-delete-btn"
        onClick={(e) => { e.stopPropagation(); onDelete(session.id); }}
        title={t('sidebar.deleteSession')}
      >×</button>
    </div>
  );
}
