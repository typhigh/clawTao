import React from 'react';

/** Shared markdown components — security: route <a> through shell.openExternal. */
export const markdownComponents = {
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
