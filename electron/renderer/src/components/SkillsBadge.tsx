import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

type SkillInfo = {
  name: string;
  description: string;
  path: string;
  source: string;
};

export function SkillsBadge({
  workspaceDir,
  onSelectSkill,
}: {
  workspaceDir?: string;
  onSelectSkill?: (skill: SkillInfo) => void;
}) {
  const { t } = useTranslation();
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [open, setOpen] = useState(false);
  const [hover, setHover] = useState(false);

  useEffect(() => {
    const refresh = () => {
      window.electronAPI.skills.list(workspaceDir).then(setSkills).catch((err) => {
        console.warn('SkillsBadge: failed to load skills', err);
      });
    };
    refresh();
    // Refresh every 5s so newly-installed skills show up without a remount.
    const id = setInterval(refresh, 5000);
    // Also refresh when the window regains focus.
    window.addEventListener('focus', refresh);
    return () => {
      clearInterval(id);
      window.removeEventListener('focus', refresh);
    };
  }, [workspaceDir]);

  if (skills.length === 0) return null;

  const sourceLabel = (s: string) => {
    if (s === 'builtin') return t('chat.skillSourceBuiltin');
    if (s === 'installed') return t('chat.skillSourceInstalled');
    return t('chat.skillSourceProject');
  };

  return (
    <div className="skills-badge" style={{ position: 'relative' }}>
      <button
        type="button"
        className="skills-badge-btn"
        onClick={(e) => {
          e.stopPropagation();
          window.electronAPI.skills.list(workspaceDir).then(setSkills).catch((err) => {
            console.warn('SkillsBadge: refresh failed', err);
          });
          setOpen((o) => !o);
        }}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => setHover(false)}
        title={t('chat.skills')}
        style={{
          appearance: 'none', WebkitAppearance: 'none',
          background: hover ? '#f0f0f0' : 'transparent',
          border: 'none',
          color: '#555',
          width: '26px', height: '26px',
          display: 'inline-flex', alignItems: 'center',
          justifyContent: 'center',
          padding: 0, font: 'inherit', fontSize: '12px',
          cursor: 'pointer', borderRadius: '6px',
          opacity: skills.length === 0 ? 0.4 : 1,
        }}
      >
        <span>{t('chat.skills')}</span>
      </button>
      {open && (
        <>
          <div
            className="skills-badge-overlay"
            onClick={() => setOpen(false)}
            style={{ position: 'fixed', inset: 0, zIndex: 99 }}
          />
          <div
            className="skills-badge-dropdown"
            style={{
              position: 'absolute', bottom: '100%', left: 0,
              marginBottom: '4px', zIndex: 100,
              background: '#fff', border: '1px solid #e0e0e0',
              borderRadius: '8px', boxShadow: '0 4px 16px rgba(0,0,0,0.1)',
              minWidth: '260px', maxWidth: '360px',
              maxHeight: '320px', overflowY: 'auto',
              padding: '4px 0',
            }}
          >
            <div style={{
              padding: '6px 12px', fontSize: '12px',
              color: '#999', borderBottom: '1px solid #f0f0f0',
            }}>
              {t('chat.availableSkills', { count: skills.length })}
            </div>
            {skills.map((s) => (
              <div
                key={s.name}
                onClick={() => {
                  onSelectSkill?.(s);
                  setOpen(false);
                }}
                title={t('chat.invokeSkill', { name: s.name })}
                style={{
                  padding: '6px 12px', fontSize: '12px',
                  lineHeight: 1.4, cursor: 'pointer',
                  color: '#777',
                }}
                onMouseEnter={(e) => {
                  const el = e.currentTarget as HTMLDivElement;
                  el.style.background = '#f5f5f5';
                  const desc = el.querySelector('.skill-desc') as HTMLElement | null;
                  if (desc) desc.style.display = 'block';
                }}
                onMouseLeave={(e) => {
                  const el = e.currentTarget as HTMLDivElement;
                  el.style.background = 'transparent';
                  const desc = el.querySelector('.skill-desc') as HTMLElement | null;
                  if (desc) desc.style.display = 'none';
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <strong style={{ color: '#333' }}>@{s.name}</strong>
                  <span style={{
                    fontSize: '10px', color: '#999',
                    background: '#f5f5f5', borderRadius: '4px',
                    padding: '1px 5px',
                  }}>
                    {sourceLabel(s.source)}
                  </span>
                </div>
                <div
                  className="skill-desc"
                  style={{ marginTop: '2px', display: 'none' }}
                >
                  {s.description}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
