import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';

// Mock ReactFlow
vi.mock('@xyflow/react', () => ({
  ReactFlow: ({ children }: { children: React.ReactNode }) =>
    React.createElement('div', { 'data-testid': 'reactflow' }, children),
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

vi.mock('../components/Toast', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn(), info: vi.fn() }),
  ToastProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement('div', null, children),
}));

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

// TemplatePicker must be importable
import TemplatePicker from '../components/TemplatePicker';
import { STARTER_TEMPLATES } from '../templates';

describe('Starter templates', () => {
  it('exports at least 3 starter templates', () => {
    expect(STARTER_TEMPLATES.length).toBeGreaterThanOrEqual(3);
  });

  it('each template has name, description, steps, and edges', () => {
    for (const tpl of STARTER_TEMPLATES) {
      expect(tpl.name).toBeTruthy();
      expect(tpl.description).toBeTruthy();
      expect(Array.isArray(tpl.steps)).toBe(true);
      expect(Array.isArray(tpl.edges)).toBe(true);
      expect(tpl.steps.length).toBeGreaterThan(0);
    }
  });

  it('ETL Pipeline template has source → transform → target', () => {
    const etl = STARTER_TEMPLATES.find((t) => /etl/i.test(t.name));
    expect(etl).toBeDefined();
    const types = etl!.steps.map((s) => s.step_type);
    expect(types).toContain('source');
    expect(types).toContain('transform');
    expect(types).toContain('target');
  });

  it('Webhook Router template has condition step', () => {
    const wh = STARTER_TEMPLATES.find((t) => /webhook/i.test(t.name));
    expect(wh).toBeDefined();
    const types = wh!.steps.map((s) => s.step_type);
    expect(types).toContain('condition');
  });

  it('CDC Replicator template has cdc_source step', () => {
    const cdc = STARTER_TEMPLATES.find((t) => /cdc/i.test(t.name));
    expect(cdc).toBeDefined();
    const types = cdc!.steps.map((s) => s.step_type);
    expect(types).toContain('cdc_source');
  });

  it('TemplatePicker renders template cards', () => {
    const onSelect = vi.fn();
    render(<TemplatePicker onSelect={onSelect} />);
    // Should show at least 3 template cards
    for (const tpl of STARTER_TEMPLATES) {
      expect(screen.getByText(tpl.name)).toBeInTheDocument();
    }
  });

  it('clicking a template card calls onSelect with template data', () => {
    const onSelect = vi.fn();
    render(<TemplatePicker onSelect={onSelect} />);
    fireEvent.click(screen.getByText(STARTER_TEMPLATES[0].name));
    expect(onSelect).toHaveBeenCalledWith(STARTER_TEMPLATES[0]);
  });

  it('TemplatePicker has a "Blank" option for starting empty', () => {
    const onSelect = vi.fn();
    render(<TemplatePicker onSelect={onSelect} />);
    expect(screen.getByText(/blank/i)).toBeInTheDocument();
    fireEvent.click(screen.getByText(/blank/i));
    expect(onSelect).toHaveBeenCalledWith(null);
  });
});
