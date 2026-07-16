import { memo, useEffect, useImperativeHandle, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useChatStore, ImageAttachment } from '../stores/chat';
import type { WorkspaceEntry } from '../stores/settings';
import { InputArea } from './InputArea';
import { ContextGrid } from './ContextGrid';
import type { ModelOption } from './ChatView';

interface Props {
  sessionId: string | null;
  isStreaming: boolean;
  thinkingEnabled: boolean;
  onToggleThinking: () => void;
  modelOptions: ModelOption[];
  selectedModelKey: string;
  onSelectModel: (key: string) => void;
  workspaceDir?: string;
  workspaceOptions?: WorkspaceEntry[];
  onSelectWorkspace?: (wsPath: string) => void;
  /** Sandbox policy per-axis overrides. */
  sandboxWrite?: 'forbidden' | 'restricted' | 'unrestricted';
  sandboxRead?: 'forbidden' | 'restricted' | 'unrestricted';
  sandboxNetwork?: 'forbidden' | 'unrestricted';
  onSandboxWriteChange?: (v: 'forbidden' | 'restricted' | 'unrestricted') => void;
  onSandboxReadChange?: (v: 'forbidden' | 'restricted' | 'unrestricted') => void;
  onSandboxNetworkChange?: (v: 'forbidden' | 'restricted' | 'unrestricted') => void;
  onCompact?: () => void;
  compactDisabled?: boolean;
  compacting?: boolean;
  messageCount?: number;
  /** Imperative handle — lets parent trigger a re-send (e.g. error retry). */
  imperativeRef?: React.Ref<{ send: () => void }>;
}

/** Input panel — owns its own state (value, images, textarea ref, auto-grow).
 *  Memoised so ChatScreen re-renders (e.g. on stream batch) don't re-render this. */
function ChatInputPanelInner({
  sessionId, isStreaming, thinkingEnabled, onToggleThinking,
  modelOptions, selectedModelKey, onSelectModel,
  workspaceDir, workspaceOptions, onSelectWorkspace,
  sandboxWrite, sandboxRead, sandboxNetwork,
  onSandboxWriteChange, onSandboxReadChange, onSandboxNetworkChange,
  onCompact, compactDisabled, compacting, messageCount,
  imperativeRef,
}: Props) {
  const [inputValue, setInputValue] = useState('');
  const [images, setImages] = useState<ImageAttachment[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Auto-resize textarea (ClawX pattern). Cap at 200px to prevent layout blow-up.
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    ta.style.height = 'auto';
    const target = Math.min(ta.scrollHeight, 200);
    ta.style.height = `${target}px`;
  }, [inputValue]);

  const handleSend = async () => {
    if (!inputValue.trim() || isStreaming || !sessionId) return;
    const text = inputValue;
    setInputValue('');
    const imgs = images.length > 0 ? [...images] : undefined;
    setImages([]);
    useChatStore.setState({ compactResult: null });
    await useChatStore.getState().sendMessage(text, thinkingEnabled, imgs);
  };

  // Stable handle that always invokes the latest handleSend (latest closures of
  // inputValue / images). Empty deps so the ref consumer doesn't re-fire.
  const handleSendRef = useRef(handleSend);
  useEffect(() => { handleSendRef.current = handleSend; });
  useImperativeHandle(imperativeRef, () => ({
    send: () => handleSendRef.current(),
  }), []);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      if (isStreaming) {
        e.preventDefault();
        return;
      }
      e.preventDefault();
      handleSend();
    }
  };

  const handleCancel = () => {
    if (sessionId) useChatStore.getState().cancelRun();
  };

  return (
    <InputArea
      value={inputValue}
      onChange={setInputValue}
      onSend={handleSend}
      onCancel={handleCancel}
      disabled={!sessionId}
      streaming={isStreaming}
      textareaRef={textareaRef}
      onKeyDown={handleKeyDown}
      modelOptions={modelOptions}
      selectedModelKey={selectedModelKey}
      onSelectModel={onSelectModel}
      thinkingEnabled={thinkingEnabled}
      onToggleThinking={onToggleThinking}
      images={images}
      onImagesChange={setImages}
      workspaceDir={workspaceDir}
      workspaceOptions={workspaceOptions}
      selectedWorkspace={workspaceDir}
      onSelectWorkspace={onSelectWorkspace}
      sandboxWrite={sandboxWrite}
      sandboxRead={sandboxRead}
      sandboxNetwork={sandboxNetwork}
      onSandboxWriteChange={onSandboxWriteChange}
      onSandboxReadChange={onSandboxReadChange}
      onSandboxNetworkChange={onSandboxNetworkChange}
      contextGrid={
        <ContextGrid
          sessionId={sessionId}
          modelKey={selectedModelKey}
          workspaceDir={workspaceDir}
          onCompact={onCompact}
          compactDisabled={compactDisabled}
          compacting={compacting}
          messageCount={messageCount}
          streaming={isStreaming}
        />
      }
    />
  );
}

export const ChatInputPanel = memo(ChatInputPanelInner);