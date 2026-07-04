import React, { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';

function CodeBlock({ children }: { children?: React.ReactNode }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    const extractText = (node: React.ReactNode): string => {
      if (typeof node === 'string') return node;
      if (Array.isArray(node)) return node.map(extractText).join('');
      if (React.isValidElement(node)) return extractText((node.props as { children?: React.ReactNode }).children);
      return '';
    };
    const text = extractText(children);
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }).catch(() => {});
  }, [children]);

  return (
    <div className="code-block-wrapper">
      <button type="button" className="code-block-copy" onClick={handleCopy}>
        {copied ? t('copied') : t('copy')}
      </button>
      <pre>{children}</pre>
    </div>
  );
}

/** Shared markdown components. Security: route <a> through shell.openExternal.
 *  i18n: CodeBlock uses its own useTranslation() so language changes propagate. */
export const markdownComponents = {
  pre: ({ children }: { children?: React.ReactNode }) => (
    <CodeBlock>{children}</CodeBlock>
  ),
  a: ({ href, children, ...rest }: { href?: string; children?: React.ReactNode }) => (
    <a
      href={href}
      {...rest}
      onClick={(e) => {
        e.preventDefault();
        if (!href) return;
        if (!/^https?:\/\//i.test(href)) return;
        window.electronAPI?.shell.openExternal(href);
      }}
    >
      {children}
    </a>
  ),
};