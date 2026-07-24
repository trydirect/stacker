// Starter DAG templates for quick-start demo
export interface StarterTemplate {
  name: string;
  description: string;
  steps: { id: string; step_type: string; name: string; config: Record<string, unknown> }[];
  edges: { source: string; target: string }[];
}

export const STARTER_TEMPLATES: StarterTemplate[] = [
  {
    name: 'ETL Pipeline',
    description: 'Classic Extract → Transform → Load pipeline',
    steps: [
      { id: 'etl-1', step_type: 'source', name: 'Fetch API Data', config: { endpoint_url: 'https://api.example.com/data', method: 'GET' } },
      { id: 'etl-2', step_type: 'transform', name: 'Clean & Map', config: { mapping_expression: '$.data.items[*]', filter_expression: '$.status == "active"' } },
      { id: 'etl-3', step_type: 'target', name: 'Write to DB', config: { endpoint_url: 'https://api.example.com/ingest', method: 'POST', batch_size: 100 } },
    ],
    edges: [
      { source: 'etl-1', target: 'etl-2' },
      { source: 'etl-2', target: 'etl-3' },
    ],
  },
  {
    name: 'Webhook Router',
    description: 'Route incoming webhooks to different targets based on conditions',
    steps: [
      { id: 'wh-1', step_type: 'http_stream_source', name: 'Webhook Receiver', config: { endpoint_url: 'https://hooks.example.com/incoming', format: 'ndjson' } },
      { id: 'wh-2', step_type: 'condition', name: 'Route by Type', config: { field: '$.event_type', operator: '==', value: 'order' } },
      { id: 'wh-3', step_type: 'target', name: 'Order Service', config: { endpoint_url: 'https://orders.example.com/api', method: 'POST' } },
      { id: 'wh-4', step_type: 'target', name: 'Notification Service', config: { endpoint_url: 'https://notify.example.com/api', method: 'POST' } },
    ],
    edges: [
      { source: 'wh-1', target: 'wh-2' },
      { source: 'wh-2', target: 'wh-3' },
      { source: 'wh-2', target: 'wh-4' },
    ],
  },
  {
    name: 'CDC Replicator',
    description: 'Capture PostgreSQL changes and replicate via gRPC',
    steps: [
      { id: 'cdc-1', step_type: 'cdc_source', name: 'PG WAL Capture', config: { connection_string: 'postgresql://user:pass@localhost/mydb', table: 'public.users', schema: 'public' } },
      { id: 'cdc-2', step_type: 'transform', name: 'Map Fields', config: { mapping_expression: '$.after.*' } },
      { id: 'cdc-3', step_type: 'grpc_target', name: 'Replicate to Target', config: { host: 'grpc.target.example.com', port: 50051, service: 'replication.Sync', method: 'PushChanges', tls: true } },
    ],
    edges: [
      { source: 'cdc-1', target: 'cdc-2' },
      { source: 'cdc-2', target: 'cdc-3' },
    ],
  },
];
