import React, { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { stepColor, STATUS_COLORS } from '../theme';

export interface DagNodeData {
  label: string;
  stepType: string;
  config: Record<string, unknown>;
  executionStatus?: string;
  error?: string;
  [key: string]: unknown;
}

const DagNode: React.FC<NodeProps> = memo(({ data }) => {
  const nodeData = data as unknown as DagNodeData;
  const color = stepColor(nodeData.stepType);
  const statusColor = nodeData.executionStatus
    ? STATUS_COLORS[nodeData.executionStatus] ?? '#9e9e9e'
    : undefined;

  return (
    <div
      style={{
        padding: '10px 16px',
        borderRadius: 8,
        border: `2px solid ${statusColor ?? color.border}`,
        background: color.bg,
        minWidth: 160,
        fontFamily: 'system-ui, sans-serif',
        boxShadow: statusColor
          ? `0 0 8px ${statusColor}40`
          : '0 1px 4px rgba(0,0,0,0.1)',
        transition: 'all 0.3s ease',
      }}
    >
      <Handle type="target" position={Position.Top} style={{ background: color.border }} />

      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
        <span style={{ fontSize: 18 }}>{color.icon}</span>
        <strong style={{ fontSize: 13, color: '#333' }}>{nodeData.label}</strong>
      </div>

      <div style={{ fontSize: 11, color: '#666', textTransform: 'uppercase', letterSpacing: 0.5 }}>
        {nodeData.stepType.replace(/_/g, ' ')}
      </div>

      {nodeData.executionStatus && (
        <div
          style={{
            marginTop: 6,
            fontSize: 10,
            fontWeight: 600,
            color: statusColor,
            textTransform: 'uppercase',
          }}
        >
          ● {nodeData.executionStatus}
        </div>
      )}

      {nodeData.error && (
        <div style={{ marginTop: 4, fontSize: 10, color: '#f44336', wordBreak: 'break-all' }}>
          {nodeData.error}
        </div>
      )}

      <Handle type="source" position={Position.Bottom} style={{ background: color.border }} />
    </div>
  );
});

DagNode.displayName = 'DagNode';

export default DagNode;
