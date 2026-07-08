import { useState, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { SendIcon, ThinkingIcon, UploadIcon } from './icons';
import type { ImageAttachment } from '../stores/chat';

const SUPPORTED_IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];

interface ModelOption {
  providerId: string;
  providerName: string;
  model: string;
  key: string;
}

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
}: Props) {
  const { t } = useTranslation();
  const [thinkingHover, setThinkingHover] = useState(false);
  const [imageHover, setImageHover] = useState(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

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

  return (
    <div className="input-area-wrapper">
      <form className="input-area" onSubmit={(e) => { e.preventDefault(); if (streaming) onCancel(); else onSend(); }}>
        <div className="input-textarea-wrap">
          <textarea
            ref={textareaRef}
            rows={3}
            placeholder={t('chat.placeholder')}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={onKeyDown}
            onPaste={handlePaste}
            disabled={disabled}
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
          {/* Hidden file input */}
          <input ref={fileInputRef} type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple hidden
            onChange={(e) => { if (e.target.files) addImages(e.target.files); e.target.value = ''; }} />
          <div className="input-bottom-row">
            <button
              type="button"
              className="input-image-btn"
              title={t('chat.attachImage')}
              onClick={() => fileInputRef.current?.click()}
              onMouseEnter={() => setImageHover(true)}
              onMouseLeave={() => setImageHover(false)}
              style={{
                appearance: 'none',
                WebkitAppearance: 'none',
                background: imageHover ? '#f0f0f0' : 'transparent',
                border: '1px solid transparent',
                color: '#555',
                borderRadius: '6px',
                width: '26px',
                height: '26px',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: 0,
                font: 'inherit',
                cursor: 'pointer',
                opacity: disabled ? 0.4 : 1,
              }}
            ><UploadIcon /></button>
            <select
              className="input-model-select input-model-select-inline"
              value={selectedModelKey}
              onChange={(e) => onSelectModel(e.target.value)}
              disabled={disabledModel || modelOptions.length === 0}
              title={t('chat.selectModel')}
            >
              {modelOptions.length === 0 ? (
                <option value="">{t('chat.noModelsConfigured')}</option>
              ) : (
                modelOptions.map(opt => (
                  <option key={opt.key} value={opt.key}>
                    {opt.providerId === 'custom' ? `${opt.providerName} / ${opt.model}` : opt.model}
                  </option>
                ))
              )}
            </select>
            <button
              type="button"
              className={'input-thinking-toggle input-thinking-toggle-inline' + (thinkingEnabled ? ' on' : '')}
              onClick={onToggleThinking}
              onMouseEnter={() => setThinkingHover(true)}
              onMouseLeave={() => setThinkingHover(false)}
              title={t('chat.thinking')}
              aria-pressed={thinkingEnabled}
              disabled={disabled}
              style={{
                appearance: 'none',
                WebkitAppearance: 'none',
                background: thinkingHover && !disabled ? '#f3f3f3' : 'transparent',
                color: thinkingEnabled ? '#1c1c1e' : '#999',
                border: '1px solid transparent',
                borderRadius: '6px',
                width: '26px',
                height: '26px',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: 0,
                cursor: disabled ? 'not-allowed' : 'pointer',
                opacity: 1,
              }}
            >
              <ThinkingIcon active={thinkingEnabled} />
            </button>
            {(workspaceOptions?.length || 0) > 0 ? (
              <select className="input-model-select input-model-select-inline"
                value={selectedWorkspace || ''}
                onChange={(e) => onSelectWorkspace?.(e.target.value)}
                title={t('chat.selectWorkspace')}
                style={{ maxWidth: '120px' }}>
                <option value="">{t('chat.noWorkspace')}</option>
                {workspaceOptions!.map(ws => (
                  <option key={ws.path} value={ws.path}>{ws.label}</option>
                ))}
              </select>
            ) : workspaceDir ? (
              <span className="input-workspace-badge" title={workspaceDir} style={{ fontSize: '11px', color: '#888', marginLeft: '6px', maxWidth: '120px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', lineHeight: '26px', fontFamily: 'monospace' }}>🏠 {workspaceDir.split('/').pop()}</span>
            ) : null}
          </div>
          {streaming ? (
            <button
              type="submit"
              className="input-send-btn input-send-btn-inline input-stop-btn"
              title={t('chat.stop')}
              style={{
                background: '#e53935',
                color: '#ffffff',
                border: 'none',
                borderRadius: '50%',
                width: '28px',
                height: '28px',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: 0,
                cursor: 'pointer',
              }}
            >
              <StopIcon />
            </button>
          ) : (
            <button
              type="submit"
              className="input-send-btn input-send-btn-inline"
              disabled={btnDisabled}
              title={t('chat.send')}
              style={{
                background: '#1c1c1e',
                color: '#ffffff',
                border: 'none',
                borderRadius: '50%',
                width: '28px',
                height: '28px',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                padding: 0,
                cursor: btnDisabled ? 'not-allowed' : 'pointer',
                opacity: btnDisabled ? 0.55 : 1,
              }}
            >
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
