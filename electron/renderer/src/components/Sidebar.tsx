import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import { SpinnerIcon, CheckIcon, PlusIcon, GearIcon, TrashIcon } from './icons';
import { formatRelative } from '../utils/format';
import type { Session } from '../stores/chat';

interface Props {
  sessions: Session[];
  activeSessionId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  onDelete: (id: string) => void;
  onOpenSettings: () => void;
  isActive?: boolean;
}

export function Sidebar({ sessions, activeSessionId, onSelect, onCreate, onDelete, onOpenSettings, isActive }: Props) {
  const { t } = useTranslation();
  const [sessionsOpen, setSessionsOpen] = useState(true);

  return (
    <aside className="sidebar">
      <div className="sidebar-new-chat">
        <button className="new-chat-btn" onClick={onCreate} title={t('sidebar.newChat')}>
          <span className="new-chat-icon"><PlusIcon /></span>
          <span className="new-chat-label">{t('sidebar.newChat')}</span>
        </button>
      </div>

      <div className="sidebar-spacer" />

      <div className="sidebar-section sessions-section">
        <button
          className="sidebar-section-header"
          onClick={() => setSessionsOpen((o) => !o)}
          title={t(sessionsOpen ? 'sidebar.collapse' : 'sidebar.expand')}
          aria-expanded={sessionsOpen}
        >
          <h2>{t('sidebar.sessions')}</h2>
          <span className="sidebar-section-toggle">
            <span className={`chevron ${sessionsOpen ? 'open' : ''}`}>›</span>
          </span>
        </button>
        {sessionsOpen && (
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
        )}
      </div>

      <div className={`sidebar-footer ${isActive ? 'active' : ''}`}>
        <button className="sidebar-footer-config" onClick={onOpenSettings} title={t('sidebar.settings')}>
          <GearIcon />
          <span>{t('sidebar.settings')}</span>
        </button>
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
      ><TrashIcon /></button>
    </div>
  );
}
