import React, { createContext, useContext, useState, useCallback, useRef, useEffect } from 'react';

type ToastType = 'success' | 'error' | 'info';

interface ToastItem {
  id: number;
  type: ToastType;
  message: string;
  autoDismiss: boolean;
}

interface ToastContextValue {
  success: (message: string) => void;
  error: (message: string) => void;
  info: (message: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

let nextId = 0;

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used inside <ToastProvider>');
  return ctx;
}

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
  }, []);

  const addToast = useCallback((type: ToastType, message: string) => {
    const id = ++nextId;
    const autoDismiss = type !== 'error';
    setToasts((prev) => [...prev, { id, type, message, autoDismiss }]);

    if (autoDismiss) {
      const timer = setTimeout(() => dismiss(id), 4000);
      timersRef.current.set(id, timer);
    }
  }, [dismiss]);

  useEffect(() => {
    return () => {
      timersRef.current.forEach((timer) => clearTimeout(timer));
    };
  }, []);

  const ctx: ToastContextValue = {
    success: (msg) => addToast('success', msg),
    error: (msg) => addToast('error', msg),
    info: (msg) => addToast('info', msg),
  };

  return (
    <ToastContext.Provider value={ctx}>
      {children}
      <ToastContainer toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
};

const TYPE_STYLES: Record<ToastType, { bg: string; border: string; icon: string }> = {
  success: { bg: '#e8f5e9', border: '#4caf50', icon: '✓' },
  error:   { bg: '#ffebee', border: '#f44336', icon: '✕' },
  info:    { bg: '#e3f2fd', border: '#2196f3', icon: 'ℹ' },
};

interface ToastContainerProps {
  toasts?: ToastItem[];
  onDismiss?: (id: number) => void;
}

export const ToastContainer: React.FC<ToastContainerProps> = ({ toasts = [], onDismiss }) => {
  return (
    <div
      style={{
        position: 'fixed',
        top: 16,
        right: 16,
        zIndex: 9999,
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        maxWidth: 380,
      }}
    >
      {toasts.map((toast) => {
        const style = TYPE_STYLES[toast.type];
        return (
          <div
            key={toast.id}
            role="alert"
            style={{
              padding: '10px 14px',
              background: style.bg,
              border: `1px solid ${style.border}`,
              borderRadius: 6,
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              fontSize: 13,
              boxShadow: '0 2px 8px rgba(0,0,0,0.15)',
              animation: 'slideIn 0.2s ease',
            }}
          >
            <span style={{ fontWeight: 700, fontSize: 14 }}>{style.icon}</span>
            <span style={{ flex: 1 }}>{toast.message}</span>
            <button
              aria-label="×"
              onClick={() => onDismiss?.(toast.id)}
              style={{
                background: 'none',
                border: 'none',
                cursor: 'pointer',
                fontSize: 16,
                padding: '0 4px',
                color: '#666',
              }}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
};
