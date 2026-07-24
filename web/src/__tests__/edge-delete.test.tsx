import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';

// Mock ReactFlow so we can inspect props passed to it
const mockReactFlow = vi.fn(({ children }: { children?: React.ReactNode }) => (
  <div data-testid="react-flow">{children}</div>
));

vi.mock('@xyflow/react', async () => {
  const actual = await vi.importActual<typeof import('@xyflow/react')>('@xyflow/react');
  return {
    ...actual,
    ReactFlow: (props: Record<string, unknown>) => {
      mockReactFlow(props);
      return <div data-testid="react-flow" />;
    },
    useNodesState: () => [[], vi.fn(), vi.fn()],
    useEdgesState: () => [[], vi.fn(), vi.fn()],
    Controls: () => null,
    MiniMap: () => null,
    Background: () => null,
  };
});

// Mock the Toast module to provide a no-op context
vi.mock('../components/Toast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  }),
  ToastProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  ToastContainer: () => null,
}));

// Mock the api module
vi.mock('../api', () => ({
  api: {
    listSteps: vi.fn().mockResolvedValue([]),
    listEdges: vi.fn().mockResolvedValue([]),
    addStep: vi.fn().mockResolvedValue({ id: 'new-1', name: 'test', step_type: 'source', config: {} }),
    updateStep: vi.fn().mockResolvedValue({}),
    deleteStep: vi.fn().mockResolvedValue(undefined),
    addEdge: vi.fn().mockResolvedValue({ id: 'e1' }),
    deleteEdge: vi.fn().mockResolvedValue(undefined),
    validateDag: vi.fn().mockResolvedValue({ valid: true }),
    executeDag: vi.fn().mockResolvedValue({ step_results: [] }),
  },
  STEP_TYPES: ['source', 'target'],
}));

import DagEditor from '../components/DagEditor';
import { api } from '../api';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('DagEditor — edge deletion', () => {
  it('passes onEdgesDelete callback to ReactFlow', () => {
    render(<DagEditor templateId="t1" token="tok" />);

    // ReactFlow should have been called with onEdgesDelete prop
    const lastCall = mockReactFlow.mock.calls[mockReactFlow.mock.calls.length - 1][0];
    expect(lastCall).toHaveProperty('onEdgesDelete');
    expect(typeof lastCall.onEdgesDelete).toBe('function');
  });

  it('passes deleteKeyCode to ReactFlow for keyboard deletion', () => {
    render(<DagEditor templateId="t1" token="tok" />);

    const lastCall = mockReactFlow.mock.calls[mockReactFlow.mock.calls.length - 1][0];
    expect(lastCall).toHaveProperty('deleteKeyCode');
  });

  it('calls api.deleteEdge when onEdgesDelete fires', async () => {
    render(<DagEditor templateId="t1" token="tok" />);

    const lastCall = mockReactFlow.mock.calls[mockReactFlow.mock.calls.length - 1][0];
    const deletedEdges = [{ id: 'edge-1', source: 's1', target: 's2' }];

    // Simulate ReactFlow calling onEdgesDelete
    lastCall.onEdgesDelete(deletedEdges);

    // api.deleteEdge should be called with the edge id
    expect(api.deleteEdge).toHaveBeenCalledWith('t1', 'edge-1', 'tok');
  });
});
