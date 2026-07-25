import { useState, useRef, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { SendIcon, ThinkingIcon, UploadIcon, GearIcon, WorkspaceIcon } from './icons';
import { ModelSelect, ModelOption } from './ModelSelect';
import { SkillsBadge } from './SkillsBadge';
import type { ImageAttachment } from '../stores/chat';

const SUPPORTED_IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];

interface Props {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  onCancel: () => void;
  disabled: boolean;
  streaming: boolean;
  sendDisabled?: boolean;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  modelOptions: ModelOption[];
  selectedModelKey: string;
  onSelectModel: (key: string) => void;
  disabledModel?: boolean;
  thinkingEnabled: boolean;
  onToggleThinking: () => void;
  images?: ImageAttachment[];
  onImagesChange?: (imgs: ImageAttachment[]) => void;
  /** Per-session workspace directory (sandbox root). */
  workspaceDir?: string;
  workspaceOptions?: import('../stores/settings').WorkspaceEntry[];
  selectedWorkspace?: string;
  onSelectWorkspace?: (wsPath: string) => void;
  /** Sandbox policy per-axis config. */
  sandboxWrite?: 'forbidden' | 'restricted' | 'unrestricted';
  sandboxRead?: 'forbidden' | 'restricted' | 'unrestricted';
  sandboxNetwork?: 'forbidden' | 'unrestricted';
  onSandboxWriteChange?: (v: 'forbidden' | 'restricted' | 'unrestricted') => void;
  onSandboxReadChange?: (v: 'forbidden' | 'restricted' | 'unrestricted') => void;
  onSandboxNetworkChange?: (v: 'forbidden' | 'restricted' | 'unrestricted') => void;
  /** Optional context-grid element rendered in the bottom row.
   *  The compact button now lives inside the context grid popover. */
  contextGrid?: React.ReactNode;
}

async function fileToImage(f: File): Promise<ImageAttachment | null> {
  if (!SUPPORTED_IMAGE_TYPES.includes(f.type)) return null;
  const buf = await f.arrayBuffer();
  const bytes = new Uint8Array(buf);
  let binary = '';
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return { base64: btoa(binary), mediaType: f.type };
}

export function InputArea({
  value, onChange, onSend, onCancel, disabled, streaming, sendDisabled, textareaRef, onKeyDown,
  modelOptions, selectedModelKey, onSelectModel, disabledModel,
  thinkingEnabled, onToggleThinking,
  images, onImagesChange,
  workspaceDir, workspaceOptions, selectedWorkspace, onSelectWorkspace,
  sandboxWrite, sandboxRead, sandboxNetwork,
  onSandboxWriteChange, onSandboxReadChange, onSandboxNetworkChange,
  contextGrid,
}: Props) {
  const { t } = useTranslation();
  const [thinkingHover, setThinkingHover] = useState(false);
  const [imageHover, setImageHover] = useState(false);
  const [gearHover, setGearHover] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  // Narrow boundary for the settings popup: only the gear + popup.
  // Clicking the textarea / model select / upload button is considered "outside"
  // and should close the popup.
  const settingsRef = useRef<HTMLDivElement | null>(null);

  // Close settings popup when clicking outside the gear/popup region.
  useEffect(() => {
    if (!settingsOpen) return;
    const handler = (e: MouseEvent) => {
      if (settingsRef.current && !settingsRef.current.contains(e.target as Node)) {
        setSettingsOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [settingsOpen]);

  // Close settings popup when the mouse leaves the gear/popup region.
  // The popup is absolutely positioned 6px above the gear, so a debounce keeps
  // it from flickering while the user travels between the two. We use a
  // generous 1s window — short enough to feel passive, long enough that
  // glancing away (e.g. at the chat scrollbar) doesn't snap it shut.
  useEffect(() => {
    if (!settingsOpen) return;
    const el = settingsRef.current;
    if (!el) return;
    let leaveTimer: number | undefined;
    const onEnter = () => {
      if (leaveTimer !== undefined) {
        window.clearTimeout(leaveTimer);
        leaveTimer = undefined;
      }
    };
    const onLeave = () => {
      leaveTimer = window.setTimeout(() => setSettingsOpen(false), 600);
    };
    el.addEventListener('mouseenter', onEnter);
    el.addEventListener('mouseleave', onLeave);
    return () => {
      el.removeEventListener('mouseenter', onEnter);
      el.removeEventListener('mouseleave', onLeave);
      if (leaveTimer !== undefined) window.clearTimeout(leaveTimer);
    };
  }, [settingsOpen]);

  // Allow Escape to dismiss the popup while it's open.
  useEffect(() => {
    if (!settingsOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSettingsOpen(false);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [settingsOpen]);

  // A workspace directory makes the "Restricted" level meaningful.  Without
  // one, Restricted collapses to Unrestricted — we hint this in the UI by
  // visually de-emphasising the option.
  const hasWorkspace = !!workspaceDir;

  // Three-dot segmented control for sandbox policies.  All three dots
  // live inside a single pill-shaped control — clicking a dot selects
  // that policy level; the label of the selected level is shown next to
  // it.  Mirrors macOS-style segmented pickers.
  //
  // `axis` identifies which policy axis this row configures ('write' /
  // 'read' / 'network').  It is used to pick the default value, not for
  // translation — translation goes through i18n keys.
  const renderPolicyRow = (
    axis: 'write' | 'read' | 'network',
    icon: string,
    value: 'forbidden' | 'restricted' | 'unrestricted' | undefined,
    onChange: (v: 'forbidden' | 'restricted' | 'unrestricted') => void,

    isDisabled: boolean,
    workspaceAvailable: boolean,
  ) => {
    const options: Array<'unrestricted' | 'restricted' | 'forbidden'> =
      axis === 'network'
        ? ['unrestricted', 'forbidden']
        : ['unrestricted', 'restricted', 'forbidden'];
    // "Write" defaults to restricted (sandboxed), others default to unrestricted.
    const defaultValue: 'unrestricted' | 'restricted' =
      axis === 'write' ? 'restricted' : 'unrestricted';
    const current = value || defaultValue;
    const axisLabel = t(`sandbox.${axis}`);
    return (
      <div className="input-settings-row">
        <span className="input-settings-icon" style={{ fontSize: '11px' }}>{icon}</span>
        <span className="input-settings-text">{axisLabel}</span>
        {/* Option column: dot control + label, all rows left-aligned to
            the same x position (the popup's option column). */}
        <div style={{
          display: 'inline-flex',
          alignItems: 'center',
          gap: '8px',
          justifySelf: 'start',
        }}>
          <div
            role="group"
            aria-label={axisLabel}
            title={
              workspaceAvailable
                ? t(`sandbox.${current}`)
                : `${t(`sandbox.${current}`)} (${t('sandbox.requiresWorkspace')})`
            }
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: '5px',
              padding: '0 8px',
              height: '22px',
              background: isDisabled ? '#f5f5f5' : '#fff',
              border: '1px solid #e0e0e0',
              borderRadius: '11px',
              opacity: isDisabled ? 0.5 : 1,
            }}
          >
            {options.map((opt) => {
              const isSelected = current === opt;
              // "Restricted" with no workspace is a no-op — soften its dot.
              const isDimmed = opt === 'restricted' && !workspaceAvailable && !isSelected;
              return (
                <button
                  key={opt}
                  type="button"
                  disabled={isDisabled}
                  onClick={(e) => {
                    if (isDisabled) return;
                    onChange(opt);
                    e.stopPropagation();
                    e.preventDefault();
                  }}
                  aria-label={t(`sandbox.${opt}`)}
                  aria-pressed={isSelected}
                  style={{
                    appearance: 'none',
                    WebkitAppearance: 'none',
                    width: '6px',
                    height: '6px',
                    borderRadius: '50%',
                    background: isSelected ? '#1c1c1e' : 'transparent',
                    border: '1.5px solid ' + (isSelected
                      ? '#1c1c1e'
                      : isDimmed ? '#ddd' : '#aaa'),
                    cursor: isDisabled ? 'not-allowed' : 'pointer',
                    padding: 0,
                    margin: 0,
                    transition: 'background 0.15s, border-color 0.15s',
                  }}
                />
              );
            })}
          </div>
          <span style={{
            fontSize: '11px',
            color: isDisabled ? '#bbb' : '#555',
            fontWeight: current === 'forbidden' ? 600 : 400,
            minWidth: '40px',
          }}>
            {t(`sandbox.${current}`)}
          </span>
        </div>
      </div>
    );
  };

  const addImages = async (files: FileList | File[]) => {
    const results: ImageAttachment[] = [];
    for (const f of files) {
      const img = await fileToImage(f);
      if (img) results.push(img);
    }
    if (results.length > 0) onImagesChange?.([...(images || []), ...results]);
  };

  const handlePaste = (e: React.ClipboardEvent) => {
    const items = e.clipboardData?.files;
    if (items?.length) { e.preventDefault(); addImages(items); }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    if (e.dataTransfer?.files.length) addImages(e.dataTransfer.files);
  };

  const removeImage = (i: number) => {
    onImagesChange?.((images || []).filter((_, idx) => idx !== i));
  };

  const btnDisabled = !value.trim() || disabled || sendDisabled;

  // Split input text into segments for @mention highlighting.
  const valueSegments = value.split(/(@[\w-]+)/g);

  return (
    <div className="input-area-wrapper">
      <form className="input-area" onSubmit={(e) => { e.preventDefault(); if (streaming) onCancel(); else onSend(); }}>
        <div className="input-textarea-wrap" style={{ position: 'relative' }}>
          {/* Mirror div — renders highlighted text behind the transparent textarea */}
          <pre
            aria-hidden="true"
            style={{
              position: 'absolute', inset: 0,
              padding: 0, margin: 0,
              overflow: 'hidden',
              whiteSpace: 'pre-wrap', wordBreak: 'break-word',
              fontFamily: 'inherit', fontSize: '14px', lineHeight: '1.5',
              color: '#000', pointerEvents: 'none',
            }}
          >
            {value && valueSegments.map((seg, i) =>
              seg.startsWith('/')
                ? <span key={i} style={{ color: '#3b82f6' }}>{seg}</span>
                : <span key={i}>{seg}</span>)}
          </pre>
          <textarea
            ref={textareaRef}
            rows={3}
            placeholder={t('chat.placeholder')}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={onKeyDown}
            onPaste={handlePaste}
            disabled={disabled}
            style={{
              position: 'relative',
              color: 'transparent',
              caretColor: '#000',
              background: 'transparent',
            }}
          />
          {/* Image thumbnails */}
          {(images?.length || 0) > 0 && (
            <div className="input-images-preview" onDrop={handleDrop} onDragOver={(e) => e.preventDefault()}>
              {images!.map((img, i) => (
                <div key={i} className="input-image-thumb">
                  <img src={`data:${img.mediaType};base64,${img.base64}`} alt="" />
                  <button className="input-image-remove" onClick={() => removeImage(i)}>✕</button>
                </div>
              ))}
            </div>
          )}
          <input ref={fileInputRef} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple hidden
            onChange={(e) => { if (e.target.files) addImages(e.target.files); e.target.value = ''; }} />
        </div>
        <div className="input-bottom-row">
          {contextGrid}
          <button
            type="button"
            className="input-image-btn"
            title={t('chat.attachImage')}
            onClick={() => fileInputRef.current?.click()}
            onMouseEnter={() => setImageHover(true)}
            onMouseLeave={() => setImageHover(false)}
            style={{ appearance: 'none', WebkitAppearance: 'none', background: imageHover ? '#f0f0f0' : 'transparent', border: '1px solid transparent', color: '#555', borderRadius: '6px', width: '26px', height: '26px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 0, font: 'inherit', cursor: 'pointer', opacity: disabled ? 0.4 : 1 }}
          ><UploadIcon /></button>
          <ModelSelect
            options={modelOptions}
            value={selectedModelKey}
            onChange={onSelectModel}
            disabled={disabledModel}
            placeholder={t('chat.noModelsConfigured')}
            title={t('chat.selectModel')}
          />
          <SkillsBadge
            workspaceDir={workspaceDir}
            onSelectSkill={(s) => onChange(value ? `${value.trimEnd()} /${s.name} ` : `/${s.name} `)}
          />
          <span style={{ flex: 1 }} />
          <div className="input-settings-wrapper" ref={settingsRef}>
            {settingsOpen && (
              <div className="input-settings-popup">
                <div className="input-settings-row">
                  <span className="input-settings-icon"><ThinkingIcon active={thinkingEnabled} /></span>
                  <span className="input-settings-text">{t('chat.thinking')}</span>
                  <button
                    type="button"
                    className={'input-thinking-toggle' + (thinkingEnabled ? ' on' : '')}
                    onClick={onToggleThinking}
                    onMouseEnter={() => setThinkingHover(true)}
                    onMouseLeave={() => setThinkingHover(false)}
                    title={t('chat.thinking')}
                    aria-pressed={thinkingEnabled}
                    disabled={disabled}
                    style={{ appearance: 'none', WebkitAppearance: 'none', background: thinkingHover && !disabled ? '#f3f3f3' : 'transparent', color: thinkingEnabled ? '#1c1c1e' : '#999', border: '1px solid #e0e0e0', borderRadius: '4px', width: '40px', height: '20px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 0, fontSize: '10px', fontWeight: 600, cursor: disabled ? 'not-allowed' : 'pointer', justifySelf: 'start' }}
                  >{thinkingEnabled ? t('chat.on') : t('chat.off')}</button>
                </div>
                {(workspaceOptions?.length || 0) > 0 ? (
                  <div className="input-settings-row">
                    <span className="input-settings-icon"><WorkspaceIcon /></span>
                    <span className="input-settings-text">{t('chat.workspace')}</span>
                    <select className="input-settings-workspace"
                      value={selectedWorkspace || ''}
                      onChange={(e) => onSelectWorkspace?.(e.target.value)}
                      title={t('chat.selectWorkspace')}
                      disabled={disabled}
                      style={{ justifySelf: 'start' }}>
                      <option value="">{t('chat.noWorkspace')}</option>
                      {workspaceOptions!.map(ws => (
                        <option key={ws.path} value={ws.path}>{ws.label}</option>
                      ))}
                    </select>
                  </div>
                ) : workspaceDir ? (
                  <div className="input-settings-row">
                    <span className="input-settings-icon"><WorkspaceIcon /></span>
                    <span className="input-settings-text">{t('chat.workspace')}</span>
                    <span className="input-workspace-badge" title={workspaceDir} style={{ fontSize: '11px', color: '#888', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontFamily: 'monospace', display: 'block', width: '100%', justifySelf: 'start' }}>{workspaceDir.split('/').pop()}</span>
                  </div>
                ) : null}
                {onSandboxWriteChange && renderPolicyRow(
                  'write', '✎', sandboxWrite, onSandboxWriteChange, disabled, hasWorkspace
                )}
                {onSandboxReadChange && renderPolicyRow(
                  'read', '☰', sandboxRead, onSandboxReadChange, disabled, hasWorkspace
                )}
                {onSandboxNetworkChange && renderPolicyRow(
                  'network', '🌐', sandboxNetwork, onSandboxNetworkChange, disabled, hasWorkspace
                )}
              </div>
            )}
            <button
              type="button"
              className={'input-settings-btn' + (settingsOpen ? ' on' : '')}
              onClick={() => setSettingsOpen(v => !v)}
              onMouseEnter={() => setGearHover(true)}
              onMouseLeave={() => setGearHover(false)}
              title={t('chat.settingsTooltip')}
              aria-pressed={settingsOpen}
              disabled={disabled}
              style={{ appearance: 'none', WebkitAppearance: 'none', background: gearHover && !disabled ? '#f3f3f3' : settingsOpen ? '#e8e8e8' : 'transparent', color: settingsOpen ? '#1c1c1e' : '#888', border: '1px solid transparent', borderRadius: '6px', width: '26px', height: '26px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 0, cursor: disabled ? 'not-allowed' : 'pointer' }}
            ><GearIcon /></button>
          </div>
          {streaming ? (
            <button type="submit" title={t('chat.stop')}
              style={{ background: '#e53935', color: '#ffffff', border: 'none', borderRadius: '50%', width: '28px', height: '28px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 0, cursor: 'pointer' }}>
              <StopIcon />
            </button>
          ) : (
            <button type="submit" disabled={btnDisabled} title={t('chat.send')}
              style={{ background: '#1c1c1e', color: '#ffffff', border: 'none', borderRadius: '50%', width: '28px', height: '28px', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', padding: 0, cursor: btnDisabled ? 'not-allowed' : 'pointer', opacity: btnDisabled ? 0.55 : 1 }}>
              <SendIcon />
            </button>
          )}
        </div>
      </form>
    </div>
  );
}

function StopIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="3" y="3" width="10" height="10" rx="2" fill="currentColor" />
    </svg>
  );
}

