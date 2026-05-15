// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Stacker DAG API client
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const BASE = '/api/v1/pipes';

export interface DagStep {
  id: string;
  pipe_template_id: string;
  name: string;
  step_type: string;
  step_order: number;
  config: Record<string, unknown>;
}

export interface DagEdge {
  id: string;
  pipe_template_id: string;
  from_step_id: string;
  to_step_id: string;
  condition?: Record<string, unknown>;
}

export interface PipeTemplate {
  id: string;
  name: string;
  description?: string;
  is_dag: boolean;
}

export interface DagExecutionResult {
  execution_id: string;
  status: string;
  total_steps: number;
  completed_steps: number;
  failed_steps: number;
  skipped_steps: number;
  execution_order: string[];
  step_results: StepResult[];
}

export interface StepResult {
  step_id: string;
  step_name: string;
  step_type: string;
  status: string;
  output_data?: Record<string, unknown>;
  error?: string;
}

export const STEP_TYPES = [
  'source', 'transform', 'condition', 'target',
  'parallel_split', 'parallel_join',
  'ws_source', 'ws_target', 'http_stream_source',
  'grpc_source', 'grpc_target', 'cdc_source',
  'amqp_source', 'kafka_source',
] as const;

export type StepType = typeof STEP_TYPES[number];

const headers = (token: string) => ({
  'Content-Type': 'application/json',
  Authorization: `Bearer ${token}`,
});

async function apiFetch<T>(url: string, opts: RequestInit): Promise<T> {
  const resp = await fetch(url, opts);
  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(`API ${resp.status}: ${text}`);
  }
  if (resp.status === 204 || opts.method === 'DELETE') {
    return undefined as T;
  }
  const json = await resp.json();
  if (json && typeof json === 'object') {
    if ('list' in json && Array.isArray(json.list)) return json.list as T;
    if ('item' in json && json.item !== undefined) return json.item as T;
  }
  return json as T;
}

export const api = {
  // Templates
  listTemplates(token: string): Promise<PipeTemplate[]> {
    return apiFetch(`${BASE}/templates`, { headers: headers(token) });
  },

  // Steps
  listSteps(templateId: string, token: string): Promise<DagStep[]> {
    return apiFetch(`${BASE}/${templateId}/dag/steps`, { headers: headers(token) });
  },
  addStep(templateId: string, step: Partial<DagStep>, token: string): Promise<DagStep> {
    return apiFetch(`${BASE}/${templateId}/dag/steps`, {
      method: 'POST',
      headers: headers(token),
      body: JSON.stringify(step),
    });
  },
  updateStep(templateId: string, stepId: string, data: Partial<DagStep>, token: string): Promise<DagStep> {
    return apiFetch(`${BASE}/${templateId}/dag/steps/${stepId}`, {
      method: 'PUT',
      headers: headers(token),
      body: JSON.stringify(data),
    });
  },
  deleteStep(templateId: string, stepId: string, token: string): Promise<void> {
    return apiFetch(`${BASE}/${templateId}/dag/steps/${stepId}`, {
      method: 'DELETE',
      headers: headers(token),
    });
  },

  // Edges
  listEdges(templateId: string, token: string): Promise<DagEdge[]> {
    return apiFetch(`${BASE}/${templateId}/dag/edges`, { headers: headers(token) });
  },
  addEdge(templateId: string, edge: { from_step_id: string; to_step_id: string; condition?: Record<string, unknown> }, token: string): Promise<DagEdge> {
    return apiFetch(`${BASE}/${templateId}/dag/edges`, {
      method: 'POST',
      headers: headers(token),
      body: JSON.stringify(edge),
    });
  },
  deleteEdge(templateId: string, edgeId: string, token: string): Promise<void> {
    return apiFetch(`${BASE}/${templateId}/dag/edges/${edgeId}`, {
      method: 'DELETE',
      headers: headers(token),
    });
  },

  // Validation & Execution
  validateDag(templateId: string, token: string): Promise<{ valid: boolean; errors?: string[] }> {
    return apiFetch(`${BASE}/${templateId}/dag/validate`, {
      method: 'POST',
      headers: headers(token),
    });
  },
  executeDag(instanceId: string, input: Record<string, unknown>, token: string): Promise<DagExecutionResult> {
    return apiFetch(`${BASE}/instances/${instanceId}/dag/execute`, {
      method: 'POST',
      headers: headers(token),
      body: JSON.stringify(input),
    });
  },
};
