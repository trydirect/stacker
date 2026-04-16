import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import React from 'react';

// Mock ReactFlow
const mockReactFlow = vi.fn((props: Record<string, unknown>) => {
  const children = props.children as React.ReactNode;
  return React.createElement('div', { 'data-testid': 'reactflow' }, children);
});
vi.mock('@xyflow/react', () => ({
  ReactFlow: (props: Record<string, unknown>) => mockReactFlow(props),
  addEdge: vi.fn((conn: unknown, edges: unknown[]) => [...edges, conn]),
  useNodesState: () => [[], vi.fn(), vi.fn()],
  useEdgesState: () => [[], vi.fn(), vi.fn()],
  Controls: () => React.createElement('div'),
  MiniMap: () => React.createElement('div'),
  Background: () => React.createElement('div'),
  BackgroundVariant: { Dots: 'dots' },
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement('div', null, children),
}));

// Mock toast
vi.mock('../components/Toast', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn(), info: vi.fn() }),
  ToastProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement('div', null, children),
}));

// Mock api — we'll spy on it
vi.mock('../api', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../api');
  return {
    ...actual,
    api: {
      listSteps: vi.fn().mockResolvedValue([]),
      listEdges: vi.fn().mockResolvedValue([]),
      addStep: vi.fn(),
      updateStep: vi.fn(),
      deleteStep: vi.fn(),
      addEdge: vi.fn(),
      deleteEdge: vi.fn(),
      validateDag: vi.fn(),
      executeDag: vi.fn(),
    },
  };
});

import DagEditor from '../components/DagEditor';
import { api } from '../api';

describe('Demo mode — standalone without auth', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows demo banner when token is empty or "dev-token"', () => {
    render(<DagEditor templateId="demo" token="" />);
    expect(screen.getByText(/demo mode/i)).toBeInTheDocument();
  });

  it('shows demo banner when token is "dev-token"', () => {
    render(<DagEditor templateId="demo" token="dev-token" />);
    expect(screen.getByText(/demo mode/i)).toBeInTheDocument();
  });

  it('does NOT show demo banner when a real token is provided', () => {
    render(<DagEditor templateId="demo" token="abc123-real-token" />);
    expect(screen.queryByText(/demo mode/i)).not.toBeInTheDocument();
  });

  it('does NOT call API to load steps in demo mode', () => {
    render(<DagEditor templateId="demo" token="" />);
    expect(api.listSteps).not.toHaveBeenCalled();
    expect(api.listEdges).not.toHaveBeenCalled();
  });

  it('validate works locally in demo mode without API call', async () => {
    render(<DagEditor templateId="demo" token="" />);
    const validateBtn = screen.getByRole('button', { name: /validate/i });
    fireEvent.click(validateBtn);
    // Should NOT have called the API validateDag
    expect(api.validateDag).not.toHaveBeenCalled();
  });

  it('execute shows demo-mode warning instead of calling API', async () => {
    render(<DagEditor templateId="demo" token="" />);
    const executeBtn = screen.getByRole('button', { name: /execute/i });
    fireEvent.click(executeBtn);
    expect(api.executeDag).not.toHaveBeenCalled();
  });
});
