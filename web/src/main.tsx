import React from 'react';
import ReactDOM from 'react-dom/client';
import { ReactFlowProvider } from '@xyflow/react';
import DagEditor from './components/DagEditor';
import { ToastProvider } from './components/Toast';

// Read template/instance/token from URL params for embedding flexibility
const params = new URLSearchParams(window.location.search);
const templateId = params.get('template') ?? 'demo';
const instanceId = params.get('instance') ?? undefined;
const token = params.get('token') ?? 'dev-token';

const App: React.FC = () => (
  <ToastProvider>
    <ReactFlowProvider>
      <DagEditor templateId={templateId} instanceId={instanceId} token={token} />
    </ReactFlowProvider>
  </ToastProvider>
);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
