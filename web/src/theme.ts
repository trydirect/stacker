import { type StepType } from './api';

// Color palette per step type
const STEP_COLORS: Record<StepType | string, { bg: string; border: string; icon: string }> = {
  source:           { bg: '#e8f5e9', border: '#4caf50', icon: '📥' },
  cdc_source:       { bg: '#e0f2f1', border: '#009688', icon: '🔄' },
  ws_source:        { bg: '#e3f2fd', border: '#2196f3', icon: '🔌' },
  grpc_source:      { bg: '#ede7f6', border: '#673ab7', icon: '⚡' },
  http_stream_source:{ bg: '#fff3e0', border: '#ff9800', icon: '🌊' },
  transform:        { bg: '#fff9c4', border: '#fbc02d', icon: '🔧' },
  condition:        { bg: '#fce4ec', border: '#e91e63', icon: '❓' },
  target:           { bg: '#ffebee', border: '#f44336', icon: '📤' },
  ws_target:        { bg: '#e8eaf6', border: '#3f51b5', icon: '📡' },
  grpc_target:      { bg: '#f3e5f5', border: '#9c27b0', icon: '🚀' },
  parallel_split:   { bg: '#e0f7fa', border: '#00bcd4', icon: '🔀' },
  parallel_join:    { bg: '#f1f8e9', border: '#8bc34a', icon: '🔁' },
  amqp_source:      { bg: '#fce4ec', border: '#e91e63', icon: '🐰' },
  kafka_source:     { bg: '#efebe9', border: '#795548', icon: '📨' },
};

export function stepColor(type: string) {
  return STEP_COLORS[type] ?? { bg: '#f5f5f5', border: '#9e9e9e', icon: '⬜' };
}

// Execution status colors
export const STATUS_COLORS: Record<string, string> = {
  pending:   '#9e9e9e',
  running:   '#2196f3',
  completed: '#4caf50',
  failed:    '#f44336',
  skipped:   '#ff9800',
};
