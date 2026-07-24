import { describe, it, expect, vi, beforeEach } from 'vitest';

// We test the api module by mocking fetch and verifying:
// 1. Field names sent to the API (name, not step_name; no position_x/y)
// 2. Response unwrapping (item/list wrappers)
// 3. Interface shape (DagStep.name, not DagStep.step_name)

// Mock fetch globally
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

// Import AFTER mocking
import { api, type DagStep } from '../api';

beforeEach(() => {
  mockFetch.mockReset();
});

describe('api.ts — field name correctness', () => {
  it('DagStep interface uses "name" not "step_name"', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ item: { id: '1', pipe_template_id: 't1', name: 'my step', step_type: 'source', step_order: 0, config: {} } }),
    });

    const step: DagStep = await api.addStep('t1', { name: 'my step', step_type: 'source' }, 'tok');
    expect(step.name).toBe('my step');
    // step_name should NOT exist on the interface
    expect((step as Record<string, unknown>).step_name).toBeUndefined();
  });

  it('addStep sends "name" field, not "step_name"', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 201,
      json: async () => ({ item: { id: '1', name: 'test', step_type: 'source', config: {} } }),
    });

    await api.addStep('tmpl-1', { name: 'test', step_type: 'source', config: {} }, 'my-token');

    const [, opts] = mockFetch.mock.calls[0];
    const body = JSON.parse(opts.body);
    expect(body).toHaveProperty('name', 'test');
    expect(body).not.toHaveProperty('step_name');
  });

  it('addStep does NOT send position_x or position_y', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 201,
      json: async () => ({ item: { id: '1', name: 'test', step_type: 'source', config: {} } }),
    });

    await api.addStep('tmpl-1', { name: 'test', step_type: 'source' }, 'tok');

    const [, opts] = mockFetch.mock.calls[0];
    const body = JSON.parse(opts.body);
    expect(body).not.toHaveProperty('position_x');
    expect(body).not.toHaveProperty('position_y');
  });

  it('updateStep sends "name" field, not "step_name"', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ item: { id: '1', name: 'updated', step_type: 'source', config: {} } }),
    });

    await api.updateStep('tmpl-1', 'step-1', { name: 'updated', config: { url: 'http://x' } }, 'tok');

    const [, opts] = mockFetch.mock.calls[0];
    const body = JSON.parse(opts.body);
    expect(body).toHaveProperty('name', 'updated');
    expect(body).not.toHaveProperty('step_name');
  });
});

describe('api.ts — response unwrapping', () => {
  it('listSteps unwraps { list: [...] } wrapper', async () => {
    const steps = [
      { id: '1', name: 's1', step_type: 'source', config: {} },
      { id: '2', name: 's2', step_type: 'target', config: {} },
    ];
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ list: steps }),
    });

    const result = await api.listSteps('tmpl-1', 'tok');
    expect(result).toEqual(steps);
    expect(Array.isArray(result)).toBe(true);
    expect(result).toHaveLength(2);
  });

  it('addStep unwraps { item: {...} } wrapper', async () => {
    const step = { id: '1', name: 'test', step_type: 'source', config: {} };
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 201,
      json: async () => ({ item: step }),
    });

    const result = await api.addStep('tmpl-1', { name: 'test', step_type: 'source' }, 'tok');
    expect(result).toEqual(step);
    expect(result.id).toBe('1');
  });

  it('listEdges unwraps { list: [...] } wrapper', async () => {
    const edges = [{ id: 'e1', from_step_id: 's1', to_step_id: 's2' }];
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ list: edges }),
    });

    const result = await api.listEdges('tmpl-1', 'tok');
    expect(result).toEqual(edges);
  });

  it('addEdge unwraps { item: {...} } wrapper', async () => {
    const edge = { id: 'e1', from_step_id: 's1', to_step_id: 's2' };
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 201,
      json: async () => ({ item: edge }),
    });

    const result = await api.addEdge('tmpl-1', { from_step_id: 's1', to_step_id: 's2' }, 'tok');
    expect(result).toEqual(edge);
  });

  it('validateDag unwraps { item: {...} } wrapper', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ item: { valid: true } }),
    });

    const result = await api.validateDag('tmpl-1', 'tok');
    expect(result).toEqual({ valid: true });
  });

  it('deleteStep handles void/204 response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 204,
      json: async () => { throw new Error('no body'); },
    });

    // Should not throw
    await expect(api.deleteStep('tmpl-1', 'step-1', 'tok')).resolves.not.toThrow();
  });

  it('throws on API error with status and message', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 401,
      text: async () => '{"message":"Unauthorized"}',
    });

    await expect(api.addStep('tmpl-1', { name: 'x', step_type: 'source' }, 'bad-tok'))
      .rejects.toThrow('API 401');
  });
});
