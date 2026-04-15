import React, { useState } from 'react';
import { stepColor } from '../theme';

interface StepConfigPanelProps {
  stepId: string;
  stepName: string;
  stepType: string;
  config: Record<string, unknown>;
  onUpdate: (stepId: string, name: string, config: Record<string, unknown>) => void;
  onDelete: (stepId: string) => void;
  onClose: () => void;
}

const StepConfigPanel: React.FC<StepConfigPanelProps> = ({
  stepId,
  stepName,
  stepType,
  config,
  onUpdate,
  onDelete,
  onClose,
}) => {
  const [name, setName] = useState(stepName);
  const [configJson, setConfigJson] = useState(JSON.stringify(config, null, 2));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const color = stepColor(stepType);

  const handleSave = () => {
    try {
      const parsed = JSON.parse(configJson);
      setJsonError(null);
      onUpdate(stepId, name, parsed);
    } catch (e) {
      setJsonError((e as Error).message);
    }
  };

  return (
    <div
      style={{
        position: 'absolute',
        top: 10,
        right: 10,
        zIndex: 10,
        background: '#fff',
        borderRadius: 8,
        boxShadow: '0 2px 12px rgba(0,0,0,0.15)',
        width: 300,
        fontFamily: 'system-ui, sans-serif',
      }}
    >
      <div
        style={{
          padding: '10px 14px',
          borderBottom: '1px solid #eee',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span>{color.icon}</span>
          <strong style={{ fontSize: 13 }}>Configure Step</strong>
        </div>
        <button
          onClick={onClose}
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            fontSize: 16,
            color: '#999',
          }}
        >
          ✕
        </button>
      </div>

      <div style={{ padding: 14 }}>
        <label style={{ fontSize: 11, fontWeight: 600, color: '#666', display: 'block', marginBottom: 4 }}>
          Name
        </label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={{
            width: '100%',
            padding: '6px 10px',
            border: '1px solid #ddd',
            borderRadius: 4,
            fontSize: 13,
            boxSizing: 'border-box',
          }}
        />

        <label
          style={{
            fontSize: 11,
            fontWeight: 600,
            color: '#666',
            display: 'block',
            marginTop: 12,
            marginBottom: 4,
          }}
        >
          Type
        </label>
        <div
          style={{
            padding: '6px 10px',
            background: color.bg,
            border: `1px solid ${color.border}40`,
            borderRadius: 4,
            fontSize: 12,
          }}
        >
          {stepType.replace(/_/g, ' ')}
        </div>

        <label
          style={{
            fontSize: 11,
            fontWeight: 600,
            color: '#666',
            display: 'block',
            marginTop: 12,
            marginBottom: 4,
          }}
        >
          Configuration (JSON)
        </label>
        <textarea
          value={configJson}
          onChange={(e) => setConfigJson(e.target.value)}
          rows={8}
          style={{
            width: '100%',
            padding: '8px 10px',
            border: `1px solid ${jsonError ? '#f44336' : '#ddd'}`,
            borderRadius: 4,
            fontSize: 12,
            fontFamily: 'monospace',
            resize: 'vertical',
            boxSizing: 'border-box',
          }}
        />
        {jsonError && (
          <div style={{ fontSize: 11, color: '#f44336', marginTop: 4 }}>{jsonError}</div>
        )}

        <div style={{ display: 'flex', gap: 8, marginTop: 14 }}>
          <button
            onClick={handleSave}
            style={{
              flex: 1,
              padding: '8px',
              background: color.border,
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
              fontSize: 12,
              fontWeight: 600,
            }}
          >
            Save
          </button>
          <button
            onClick={() => onDelete(stepId)}
            style={{
              padding: '8px 12px',
              background: '#f44336',
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
              fontSize: 12,
            }}
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  );
};

export default StepConfigPanel;
