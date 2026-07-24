import React from 'react';

interface ToolbarProps {
  templateId: string;
  onValidate: () => void;
  onExecute: () => void;
  validationResult?: { valid: boolean; errors?: string[] } | null;
  executing: boolean;
}

const Toolbar: React.FC<ToolbarProps> = ({
  templateId,
  onValidate,
  onExecute,
  validationResult,
  executing,
}) => (
  <div
    style={{
      position: 'absolute',
      bottom: 10,
      left: '50%',
      transform: 'translateX(-50%)',
      zIndex: 10,
      background: '#fff',
      borderRadius: 8,
      boxShadow: '0 2px 12px rgba(0,0,0,0.15)',
      display: 'flex',
      alignItems: 'center',
      gap: 8,
      padding: '8px 14px',
      fontFamily: 'system-ui, sans-serif',
    }}
  >
    <span style={{ fontSize: 11, color: '#999', maxWidth: 120, overflow: 'hidden', textOverflow: 'ellipsis' }}>
      {templateId.slice(0, 8)}…
    </span>

    <button
      onClick={onValidate}
      style={{
        padding: '6px 14px',
        background: '#2196f3',
        color: '#fff',
        border: 'none',
        borderRadius: 4,
        cursor: 'pointer',
        fontSize: 12,
        fontWeight: 600,
      }}
    >
      ✓ Validate
    </button>

    <button
      onClick={onExecute}
      disabled={executing}
      style={{
        padding: '6px 14px',
        background: executing ? '#9e9e9e' : '#4caf50',
        color: '#fff',
        border: 'none',
        borderRadius: 4,
        cursor: executing ? 'not-allowed' : 'pointer',
        fontSize: 12,
        fontWeight: 600,
      }}
    >
      {executing ? '⏳ Running…' : '▶ Execute'}
    </button>

    {validationResult && (
      <span
        style={{
          fontSize: 11,
          color: validationResult.valid ? '#4caf50' : '#f44336',
          fontWeight: 600,
        }}
      >
        {validationResult.valid
          ? '✓ Valid'
          : `✗ ${validationResult.errors?.[0] ?? 'Invalid'}`}
      </span>
    )}
  </div>
);

export default Toolbar;
