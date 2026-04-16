import React, { useState, useEffect } from 'react';
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

// Field definitions per step type
interface FieldDef {
  key: string;
  label: string;
  type: 'text' | 'select' | 'number' | 'checkbox';
  options?: string[];
  placeholder?: string;
}

const STEP_FIELDS: Record<string, FieldDef[]> = {
  source: [
    { key: 'endpoint_url', label: 'Endpoint URL', type: 'text', placeholder: 'https://api.example.com/data' },
    { key: 'method', label: 'Method', type: 'select', options: ['GET', 'POST'] },
    { key: 'headers', label: 'Headers (JSON)', type: 'text', placeholder: '{"Authorization":"Bearer ..."}' },
    { key: 'polling_interval', label: 'Polling Interval (s)', type: 'number' },
  ],
  transform: [
    { key: 'mapping_expression', label: 'Mapping Expression', type: 'text', placeholder: '$.data.items[*]' },
    { key: 'filter_expression', label: 'Filter Expression', type: 'text', placeholder: '$.status == "active"' },
  ],
  condition: [
    { key: 'field', label: 'Field', type: 'text', placeholder: '$.status' },
    { key: 'operator', label: 'Operator', type: 'select', options: ['==', '!=', '>', '<', '>=', '<=', 'contains'] },
    { key: 'value', label: 'Value', type: 'text', placeholder: 'active' },
  ],
  target: [
    { key: 'endpoint_url', label: 'Endpoint URL', type: 'text', placeholder: 'https://api.example.com/ingest' },
    { key: 'method', label: 'Method', type: 'select', options: ['POST', 'PUT', 'PATCH'] },
    { key: 'headers', label: 'Headers (JSON)', type: 'text', placeholder: '{"Content-Type":"application/json"}' },
    { key: 'batch_size', label: 'Batch Size', type: 'number' },
  ],
  ws_source: [
    { key: 'ws_url', label: 'WebSocket URL', type: 'text', placeholder: 'wss://stream.example.com/ws' },
    { key: 'reconnect_interval', label: 'Reconnect Interval (s)', type: 'number' },
  ],
  ws_target: [
    { key: 'ws_url', label: 'WebSocket URL', type: 'text', placeholder: 'wss://target.example.com/ws' },
    { key: 'reconnect_interval', label: 'Reconnect Interval (s)', type: 'number' },
  ],
  grpc_source: [
    { key: 'host', label: 'Host', type: 'text', placeholder: 'grpc.example.com' },
    { key: 'port', label: 'Port', type: 'number' },
    { key: 'service', label: 'Service', type: 'text', placeholder: 'my.package.MyService' },
    { key: 'method', label: 'Method', type: 'text', placeholder: 'StreamData' },
    { key: 'tls', label: 'TLS', type: 'checkbox' },
  ],
  grpc_target: [
    { key: 'host', label: 'Host', type: 'text', placeholder: 'grpc.example.com' },
    { key: 'port', label: 'Port', type: 'number' },
    { key: 'service', label: 'Service', type: 'text', placeholder: 'my.package.MyService' },
    { key: 'method', label: 'Method', type: 'text', placeholder: 'IngestData' },
    { key: 'tls', label: 'TLS', type: 'checkbox' },
  ],
  http_stream_source: [
    { key: 'endpoint_url', label: 'Endpoint URL', type: 'text', placeholder: 'https://api.example.com/stream' },
    { key: 'format', label: 'Format', type: 'select', options: ['sse', 'chunked', 'ndjson'] },
  ],
  cdc_source: [
    { key: 'connection_string', label: 'Connection String', type: 'text', placeholder: 'postgresql://user:pass@host/db' },
    { key: 'table', label: 'Table', type: 'text', placeholder: 'public.users' },
    { key: 'schema', label: 'Schema', type: 'text', placeholder: 'public' },
  ],
  parallel_split: [
    { key: 'strategy', label: 'Strategy', type: 'select', options: ['round_robin', 'broadcast', 'hash'] },
    { key: 'max_parallel', label: 'Max Parallel', type: 'number' },
  ],
  parallel_join: [
    { key: 'strategy', label: 'Strategy', type: 'select', options: ['wait_all', 'wait_any', 'merge'] },
  ],
};

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
  const [formData, setFormData] = useState<Record<string, unknown>>({ ...config });
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [configJson, setConfigJson] = useState(JSON.stringify(config, null, 2));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const color = stepColor(stepType);
  const fields = STEP_FIELDS[stepType] ?? [];

  useEffect(() => {
    setFormData({ ...config });
    setConfigJson(JSON.stringify(config, null, 2));
  }, [config]);

  const handleFieldChange = (key: string, value: unknown) => {
    setFormData((prev) => ({ ...prev, [key]: value }));
  };

  const handleSave = () => {
    if (showAdvanced) {
      try {
        const parsed = JSON.parse(configJson);
        setJsonError(null);
        onUpdate(stepId, name, parsed);
      } catch (e) {
        setJsonError((e as Error).message);
      }
    } else {
      onUpdate(stepId, name, formData);
    }
  };

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: '6px 10px',
    border: '1px solid #ddd',
    borderRadius: 4,
    fontSize: 13,
    boxSizing: 'border-box',
  };

  const labelStyle: React.CSSProperties = {
    fontSize: 11,
    fontWeight: 600,
    color: '#666',
    display: 'block',
    marginTop: 10,
    marginBottom: 4,
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
        maxHeight: '90vh',
        overflowY: 'auto',
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
          style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 16, color: '#999' }}
        >
          ✕
        </button>
      </div>

      <div style={{ padding: 14 }}>
        <label style={{ ...labelStyle, marginTop: 0 }}>Name</label>
        <input value={name} onChange={(e) => setName(e.target.value)} style={inputStyle} />

        <label style={labelStyle}>Type</label>
        <div
          style={{
            padding: '6px 10px',
            background: color.bg,
            border: `1px solid ${color.border}40`,
            borderRadius: 4,
            fontSize: 12,
            color: '#555',
            fontStyle: 'italic',
          }}
        >
          {stepType.replace(/_/g, ' ')}
        </div>

        {/* Structured form fields */}
        {!showAdvanced && fields.map((field) => (
          <div key={field.key}>
            <label htmlFor={`field-${field.key}`} style={labelStyle}>
              {field.label}
            </label>
            {field.type === 'select' ? (
              <select
                id={`field-${field.key}`}
                value={String(formData[field.key] ?? field.options?.[0] ?? '')}
                onChange={(e) => handleFieldChange(field.key, e.target.value)}
                style={inputStyle}
              >
                {field.options?.map((opt) => (
                  <option key={opt} value={opt}>{opt}</option>
                ))}
              </select>
            ) : field.type === 'checkbox' ? (
              <input
                id={`field-${field.key}`}
                type="checkbox"
                checked={Boolean(formData[field.key])}
                onChange={(e) => handleFieldChange(field.key, e.target.checked)}
              />
            ) : field.type === 'number' ? (
              <input
                id={`field-${field.key}`}
                type="number"
                value={formData[field.key] != null ? String(formData[field.key]) : ''}
                onChange={(e) => handleFieldChange(field.key, e.target.value ? Number(e.target.value) : undefined)}
                placeholder={field.placeholder}
                style={inputStyle}
              />
            ) : (
              <input
                id={`field-${field.key}`}
                type="text"
                value={String(formData[field.key] ?? '')}
                onChange={(e) => handleFieldChange(field.key, e.target.value)}
                placeholder={field.placeholder}
                style={inputStyle}
              />
            )}
          </div>
        ))}

        {/* Advanced JSON toggle */}
        <div
          onClick={() => {
            if (!showAdvanced) setConfigJson(JSON.stringify(formData, null, 2));
            setShowAdvanced(!showAdvanced);
          }}
          style={{
            marginTop: 12,
            fontSize: 11,
            color: '#2196f3',
            cursor: 'pointer',
            userSelect: 'none',
          }}
        >
          {showAdvanced ? '◀ Structured Form' : 'Advanced JSON ▶'}
        </div>

        {showAdvanced && (
          <>
            <textarea
              aria-label="json"
              value={configJson}
              onChange={(e) => setConfigJson(e.target.value)}
              rows={8}
              style={{
                width: '100%',
                marginTop: 6,
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
          </>
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
