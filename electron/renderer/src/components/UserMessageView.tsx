export function UserMessageView({ content }: { content: string }) {
  return (
    <div className="message user">
      <div className="message-content">{content}</div>
    </div>
  );
}
