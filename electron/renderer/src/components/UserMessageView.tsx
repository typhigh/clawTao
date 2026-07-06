import { memo } from 'react';
import type { ImageAttachment } from '../stores/chat';

export const UserMessageView = memo(function UserMessageView({ content, images }: {
  content: string;
  images?: ImageAttachment[];
}) {
  return (
    <div className="message user">
      {(images?.length || 0) > 0 && (
        <div className="user-images">
          {images!.map((img, i) => (
            <img key={i} className="user-image-thumb" src={`data:${img.mediaType};base64,${img.base64}`} alt="" />
          ))}
        </div>
      )}
      <div className="message-content">{content}</div>
    </div>
  );
});
