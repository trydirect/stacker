import React, { useState } from 'react';
import { type StepType } from '../api';
import { stepColor } from '../theme';

interface StepPaletteProps {
  onAddStep: (stepType: StepType, name: string) => void;
}

const StepPalette: React.FC<StepPaletteProps> = ({ onAddStep }) => {
  const [expanded, setExpanded] = useState(true);

  const categories: Record<string, StepType[]> = {
    'Sources': ['source', 'cdc_source', 'ws_source', 'grpc_source', 'http_stream_source', 'amqp_source', 'kafka_source'],
    'Processing': ['transform', 'condition'],
    'Targets': ['target', 'ws_target', 'grpc_target'],
    'Parallel': ['parallel_split', 'parallel_join'],
  };

  return (
    <div
      style={{
        position: 'absolute',
        top: 10,
        left: 10,
        zIndex: 10,
        background: '#fff',
        borderRadius: 8,
        boxShadow: '0 2px 12px rgba(0,0,0,0.15)',
        width: expanded ? 200 : 40,
        transition: 'width 0.2s ease',
        overflow: 'hidden',
      }}
    >
      <div
        onClick={() => setExpanded((e) => !e)}
        style={{
          padding: '8px 12px',
          cursor: 'pointer',
          fontWeight: 600,
          fontSize: 13,
          borderBottom: expanded ? '1px solid #eee' : 'none',
          display: 'flex',
          alignItems: 'center',
          gap: 6,
        }}
      >
        {expanded ? '◀ Steps' : '▶'}
      </div>

      {expanded &&
        Object.entries(categories).map(([category, types]) => (
          <div key={category} style={{ padding: '4px 0' }}>
            <div
              style={{
                padding: '4px 12px',
                fontSize: 10,
                fontWeight: 600,
                color: '#999',
                textTransform: 'uppercase',
                letterSpacing: 0.8,
              }}
            >
              {category}
            </div>
            {types.map((type) => {
              const color = stepColor(type);
              return (
                <div
                  key={type}
                  draggable
                  onDragStart={(e) => {
                    e.dataTransfer.setData('stepType', type);
                    e.dataTransfer.effectAllowed = 'move';
                  }}
                  onClick={() => onAddStep(type, type.replace(/_/g, ' '))}
                  style={{
                    padding: '6px 12px',
                    margin: '2px 8px',
                    borderRadius: 4,
                    background: color.bg,
                    border: `1px solid ${color.border}40`,
                    cursor: 'grab',
                    fontSize: 12,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    transition: 'transform 0.1s',
                  }}
                  onMouseDown={(e) =>
                    ((e.target as HTMLElement).style.transform = 'scale(0.97)')
                  }
                  onMouseUp={(e) =>
                    ((e.target as HTMLElement).style.transform = 'scale(1)')
                  }
                >
                  <span>{color.icon}</span>
                  <span>{type.replace(/_/g, ' ')}</span>
                </div>
              );
            })}
          </div>
        ))}
    </div>
  );
};

export default StepPalette;
