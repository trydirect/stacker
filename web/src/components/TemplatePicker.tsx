import React from 'react';
import { STARTER_TEMPLATES, type StarterTemplate } from '../templates';
import { stepColor } from '../theme';

interface TemplatePickerProps {
  onSelect: (template: StarterTemplate | null) => void;
}

const TemplatePicker: React.FC<TemplatePickerProps> = ({ onSelect }) => (
  <div
    style={{
      position: 'absolute',
      inset: 0,
      zIndex: 30,
      background: 'rgba(0,0,0,0.4)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      fontFamily: 'system-ui, sans-serif',
    }}
  >
    <div
      style={{
        background: '#fff',
        borderRadius: 12,
        boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
        padding: 28,
        maxWidth: 640,
        width: '90%',
      }}
    >
      <h2 style={{ margin: '0 0 6px', fontSize: 20 }}>Choose a Template</h2>
      <p style={{ margin: '0 0 20px', fontSize: 13, color: '#666' }}>
        Start with a pre-built pipeline or create one from scratch.
      </p>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
        {STARTER_TEMPLATES.map((tpl) => {
          const types = [...new Set(tpl.steps.map((s) => s.step_type))];
          return (
            <div
              key={tpl.name}
              onClick={() => onSelect(tpl)}
              style={{
                border: '1px solid #e0e0e0',
                borderRadius: 8,
                padding: 14,
                cursor: 'pointer',
                transition: 'box-shadow 0.15s',
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLDivElement).style.boxShadow = '0 2px 12px rgba(0,0,0,0.1)';
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLDivElement).style.boxShadow = 'none';
              }}
            >
              <strong style={{ fontSize: 14 }}>{tpl.name}</strong>
              <p style={{ fontSize: 12, color: '#666', margin: '4px 0 8px' }}>{tpl.description}</p>
              <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>
                {types.map((t) => (
                  <span
                    key={t}
                    style={{
                      fontSize: 10,
                      padding: '2px 6px',
                      borderRadius: 4,
                      background: stepColor(t).bg,
                      color: stepColor(t).border,
                      border: `1px solid ${stepColor(t).border}40`,
                    }}
                  >
                    {stepColor(t).icon} {t}
                  </span>
                ))}
              </div>
            </div>
          );
        })}

        {/* Blank option */}
        <div
          onClick={() => onSelect(null)}
          style={{
            border: '2px dashed #ccc',
            borderRadius: 8,
            padding: 14,
            cursor: 'pointer',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexDirection: 'column',
            color: '#999',
            transition: 'border-color 0.15s',
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLDivElement).style.borderColor = '#666';
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLDivElement).style.borderColor = '#ccc';
          }}
        >
          <span style={{ fontSize: 24, marginBottom: 4 }}>✚</span>
          <strong style={{ fontSize: 14 }}>Blank</strong>
          <p style={{ fontSize: 12, margin: '4px 0 0' }}>Start from scratch</p>
        </div>
      </div>
    </div>
  </div>
);

export default TemplatePicker;
