import React from 'react';
import { createRoot } from 'react-dom/client';
import './i18n';
import App from './components/App';
import './styles.css';

const container = document.getElementById('root');
if (container) {
  const root = createRoot(container);
  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}
