# **`Technical Requirements V2:`** 

# **`Stacker improvement`**

## **`2. Extended System Architecture`**

The goal is to extend current system with the new modules and services to support advanced command processing, real-time communication, and multi-tenant isolation. Basically, we are adding new components for communication with deployed agents, command queuing, and  some basic metrics collection. 

### **`2.1 High-Level Architecture`**

`text`  
`┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐`  
`│   Web Frontend  │    │   API Gateway   │    │  Auth Service   │`  
`│   (Dashboard)   │◀──▶│   (Load Balancer)│◀──▶│  (JWT/OAuth)   │`  
`└─────────────────┘    └─────────────────┘    └─────────────────┘`  
                              `│`  
        `┌─────────────────────┼─────────────────────┐`  
        `│                     │                     │`  
        `▼                     ▼                     ▼`  
`┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐`  
`│ Command Service │  │   Metrics API   │  │   WebSocket     │`  
`│ (HTTP Long Poll)│  │   (InfluxDB)    │  │   Gateway       │`  
`└─────────────────┘  └─────────────────┘  └─────────────────┘`  
        `│                     │                     │`  
        `▼                     ▼                     ▼`  
`┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐`  
`│  Command Queue  │  │  Metrics Store  │  │  Agent Registry │`  
`│   (PostgreSQL)  │  │   (InfluxDB)    │  │   (Redis)       │`  
`└─────────────────┘  └─────────────────┘  └─────────────────┘`  
                              `│                     │`  
                              `└─────────────────────┘`  
                                      `│`  
                                      `▼`  
                               `┌─────────────────┐`  
                               `│    Agents       │`  
                               `│   (deployed)    │`  
                               `└─────────────────┘`

### **`2.2 Component Overview`**

#### **`Core Services:`**

1. **`Command Service`** `- HTTP Long Polling endpoint for agent communication`  
2. **`WebSocket Gateway`** `- Real-time bidirectional communication`  
3. **`Metrics Service`** `- Time-series data collection and querying`  
4. **`Authentication Service`** `- Multi-tenant user management`  
5. **`Audit Service`** `- Command logging and compliance tracking`  
6. **`Notification Service`** `- Real-time user notifications`

#### **`Data Stores:`**

1. **`PostgreSQL`** `- Relational data (deployments, commands)`  
2. **`InfluxDB`** `- Time-series metrics and monitoring data`  
3. **`Redis`** `- Caching, sessions, and agent state`  
4. **`Object Storage`** `- Backup storage, log archives`

## **`3. API Specification`**

### **`3.1 Command API Endpoints`**

#### **`3.1.1 Agent-facing Endpoints (Long Polling)`**

`text`  
`# Agent Command Polling`  
`GET /api/v1/agent/commands/wait/{deployment_hash}`  
`Headers:`  
  `Authorization: Bearer {agent_token}`  
  `X-Agent-Version: {version}`  
`Query Parameters:`  
  `timeout: 30 (seconds, max 120)`  
  `priority: normal|high|critical`  
  `last_command_id: {id} (for deduplication)`

`Response:`  
  `200 OK: { "command": CommandObject }`  
  `204 No Content: No commands available`  
  `401 Unauthorized: Invalid token`  
  `410 Gone: Agent decommissioned`

`# Agent Result Reporting`  
`POST /api/v1/agent/commands/report`  
`Headers:`  
  `Authorization: Bearer {agent_token}`  
  `Content-Type: application/json`  
`Body: CommandResult`

`Response:`  
  `200 OK: Result accepted`  
  `202 Accepted: Result queued for processing`  
  `400 Bad Request: Invalid result format`

`# Agent Registration`   

`POST /api/v1/agent/register`  
`Headers:`  
  `X-Agent-Signature: {signature}`  
`Body:`  
  `{`  
    `"deployment_hash": "abc123",`  
    `"public_key": "-----BEGIN PUBLIC KEY-----\n...",`  
    `"capabilities": ["backup", "monitoring", "updates"],`  
    `"system_info": { ... },`  
    `"agent_version": "1.0.0"`  
  `}`

`Response:`  
  `201 Created:`  
  `{`  
    `"agent_token": "jwt_token",`  
    `"dashboard_version": "2.1.0",`  
    `"supported_api_versions": ["1.0", "1.1"],`  
    `"config_endpoint": "/api/v1/agent/config"`  
  `}`

#### **`3.1.2 User-facing Endpoints`**

`text`  
`# Create Command`  
`POST /api/v1/users/{user_id}/deployments/{deployment_hash}/commands`  
`Headers:`  
  `Authorization: Bearer {user_token}`  
`Body:`  
  `{`  
    `"type": "application.update",`  
    `"parameters": { ... },`  
    `"priority": "normal",`  
    `"schedule_at": "2024-01-15T10:30:00Z",`  
    `"requires_confirmation": true`  
  `}`

`Response:`  
  `202 Accepted:`  
  `{`  
    `"command_id": "cmd_abc123",`  
    `"status": "queued",`  
    `"estimated_start": "2024-01-15T10:30:00Z"`  
  `}`

`# List Commands`  
`GET /api/v1/users/{user_id}/deployments/{deployment_hash}/commands`  
`Query Parameters:`  
  `status: queued|executing|completed|failed`  
  `limit: 50`  
  `offset: 0`  
  `from_date: 2024-01-01`  
  `to_date: 2024-01-31`

`# Get Command Status`  
`GET /api/v1/users/{user_id}/deployments/{deployment_hash}/commands/{command_id}`

`# Cancel Command`  
`POST /api/v1/users/{user_id}/deployments/{deployment_hash}/commands/{command_id}/cancel`

### **`3.2 Metrics API Endpoints`**

`text`  
`# Query Metrics (Prometheus format)`  
`GET /api/v1/metrics/query`  
`Query Parameters:`  
  `query: 'cpu_usage{deployment_hash="abc123"}'`  
  `time: 1705305600`  
  `step: 30s`

`# Range Query`  
`GET /api/v1/metrics/query_range`  
`Query Parameters:`  
  `query: 'cpu_usage{deployment_hash="abc123"}'`  
  `start: 1705305600`  
  `end: 1705309200`  
  `step: 30s`

`# Write Metrics (Agent → Dashboard)`  
`POST /api/v1/metrics/write`  
`Headers:`  
  `Authorization: Bearer {agent_token}`  
`Body: InfluxDB line protocol or JSON`

### **`3.3 WebSocket Endpoints`**

`text`  
`# Agent Connection`  
`wss://dashboard.try.direct/ws/agent/{deployment_hash}`  
`Authentication: Bearer token in query string`

`# User Dashboard Connection`  
`wss://dashboard.try.direct/ws/user/{user_id}`  
`Authentication: Bearer token in query string`

`# Real-time Event Types:`  
`- command_progress: {command_id, progress, stage}`  
`- command_completed: {command_id, result, status}`  
`- system_alert: {type, severity, message}`  
`- log_entry: {timestamp, level, message, source}`  
`- agent_status: {status, last_seen, metrics}`

## **`4. Data Models`**

### **`4.1 Core Entities`**

`typescript`  
`// Deployment Model`  
`interface Deployment {`  
  `id: string;`  
  `deployment_hash: string;`  
  `user_id: string;`  
  `agent_id: string;`  
  `status: 'active' | 'inactive' | 'suspended';`  
  `created_at: Date;`  
  `last_seen_at: Date;`  
  `metadata: {`  
    `application_type: string;`  
    `server_size: string;`  
    `region: string;`  
    `tags: string[];`  
  `};`  
`}`

`// Command Model`  
`interface Command {`  
  `id: string;`  
  `deployment_hash: string;`  
  `type: CommandType;`  
  `status: 'queued' | 'sent' | 'executing' | 'completed' | 'failed' | 'cancelled';`  
  `priority: 'low' | 'normal' | 'high' | 'critical';`  
  `parameters: Record<string, any>;`  
  `created_by: string;`  
  `created_at: Date;`  
  `scheduled_for: Date;`  
  `sent_at: Date;`  
  `started_at: Date;`  
  `completed_at: Date;`  
  `timeout_seconds: number;`  
  `result?: CommandResult;`  
  `error?: CommandError;`  
  `metadata: {`  
    `requires_confirmation: boolean;`  
    `rollback_on_failure: boolean;`  
    `estimated_duration: number;`  
    `checkpoint_support: boolean;`  
  `};`  
`}`

`// Agent Model`  
`interface Agent {`  
  `id: string;`  
  `deployment_hash: string;`  
  `status: 'online' | 'offline' | 'degraded';`  
  `last_heartbeat: Date;`  
  `capabilities: string[];`  
  `version: string;`  
  `system_info: {`  
    `os: string;`  
    `architecture: string;`  
    `memory_mb: number;`  
    `cpu_cores: number;`  
  `};`  
  `connection_info: {`  
    `ip_address: string;`  
    `latency_ms: number;`  
    `last_command_id: string;`  
  `};`  
`}`

### **`4.2 Database Schema`**

`sql`  
`-- PostgreSQL Schema`

`-- Users & Tenants`  
`CREATE TABLE tenants (`  
  `id UUID PRIMARY KEY,`  
  `name VARCHAR(255) NOT NULL,`  
  `plan VARCHAR(50) NOT NULL,`  
  `settings JSONB DEFAULT '{}',`  
  `created_at TIMESTAMP DEFAULT NOW()`  
`);`


`-- Deployments`  

`UPDATE TABLE deployment (`  
add following new fields
  `deployment_hash VARCHAR(64) UNIQUE NOT NULL,`  
  `tenant_id UUID REFERENCES tenants(id),`  
  `user_id ,` -- taken from remote api --  
  `last_seen_at TIMESTAMP DEFAULT NOW()` -- updated on each heartbeat, when agent was online last time --  
  Rename body field to `metadata`
  `metadata JSONB DEFAULT '{}',`  
`);`

`-- Agents`  
`CREATE TABLE agents (`  
  `id UUID PRIMARY KEY,`  
  `deployment_hash VARCHAR(64) REFERENCES deployments(deployment_hash),`  
  `agent_token VARCHAR(255) UNIQUE NOT NULL,`  
  `public_key TEXT,`  
  `capabilities JSONB DEFAULT '[]',`  
  `version VARCHAR(50),`  
  `system_info JSONB DEFAULT '{}',`  
  `last_heartbeat TIMESTAMP,`  
  `status VARCHAR(50) DEFAULT 'offline',`  
  `created_at TIMESTAMP DEFAULT NOW()`  
`);`

`-- Commands`  
`CREATE TABLE commands (`  
  `id UUID PRIMARY KEY,`  
  `command_id VARCHAR(64) UNIQUE NOT NULL,`  
  `deployment_hash VARCHAR(64) REFERENCES deployments(deployment_hash),`  
  `type VARCHAR(100) NOT NULL,`  
  `status VARCHAR(50) DEFAULT 'queued',`  
  `priority VARCHAR(20) DEFAULT 'normal',`  
  `parameters JSONB DEFAULT '{}',`  
  `result JSONB,`  
  `error JSONB,`  
  `created_by UUID REFERENCES users(id),`  
  `created_at TIMESTAMP DEFAULT NOW(),`  
  `scheduled_for TIMESTAMP,`  
  `sent_at TIMESTAMP,`  
  `started_at TIMESTAMP,`  
  `completed_at TIMESTAMP,`  
  `timeout_seconds INTEGER DEFAULT 300,`  
  `metadata JSONB DEFAULT '{}',`  
  `CHECK (status IN ('queued', 'sent', 'executing', 'completed', 'failed', 'cancelled')),`  
  `CHECK (priority IN ('low', 'normal', 'high', 'critical'))`  
`);`

`-- Command Queue (for long polling)`  
`CREATE TABLE command_queue (`  
  `id UUID PRIMARY KEY,`  
  `command_id UUID REFERENCES commands(id),`  
  `deployment_hash VARCHAR(64),`  
  `priority INTEGER DEFAULT 0,`  
  `created_at TIMESTAMP DEFAULT NOW(),`  
  `INDEX idx_queue_deployment (deployment_hash, priority, created_at)`  
`);`

`-- Audit Log`  
`CREATE TABLE audit_log (`  
  `id UUID PRIMARY KEY,`  
  `tenant_id UUID REFERENCES tenants(id),`  
  `user_id UUID REFERENCES users(id),`  
  `action VARCHAR(100) NOT NULL,`  
  `resource_type VARCHAR(50),`  
  `resource_id VARCHAR(64),`  
  `details JSONB DEFAULT '{}',`  
  `ip_address INET,`  
  `user_agent TEXT,`  
  `created_at TIMESTAMP DEFAULT NOW()`  
`);`

`-- Metrics Metadata`  
`CREATE TABLE metric_metadata (`  
  `id UUID PRIMARY KEY,`  
  `deployment_hash VARCHAR(64) REFERENCES deployments(deployment_hash),`  
  `metric_name VARCHAR(255) NOT NULL,`  
  `description TEXT,`  
  `unit VARCHAR(50),`  
  `aggregation_type VARCHAR(50),`  
  `retention_days INTEGER DEFAULT 30,`  
  `created_at TIMESTAMP DEFAULT NOW(),`  
  `UNIQUE(deployment_hash, metric_name)`  
`);`

## **`5. Command Processing Pipeline`**

### **`5.1 Command Flow Sequence`**

`text`  
`1. User creates command via Dashboard/API`  
   `→ Command stored in PostgreSQL with status='queued'`  
   `→ Event published to message queue`

`2. Command Scheduler processes event`  
   `→ Validates command parameters`  
   `→ Checks agent capabilities`  
   `→ Adds to command_queue table with priority`

`3. Agent polls via HTTP Long Polling`  
   `→ Server checks command_queue for agent's deployment_hash`  
   `→ If command exists:`  
        `• Updates command status='sent'`  
        `• Records sent_at timestamp`  
        `• Removes from command_queue`  
        `• Returns command to agent`  
   `→ If no command:`  
        `• Holds connection for timeout period`  
        `• Returns 204 No Content on timeout`

`4. Agent executes command and reports result`  
   `→ POST to /commands/report endpoint`  
   `→ Server validates agent token`  
   `→ Updates command status='completed' or 'failed'`  
   `→ Stores result/error`  
   `→ Publishes completion event`

`5. Real-time notifications`  
   `→ WebSocket Gateway sends update to user's dashboard`  
   `→ Notification Service sends email/Slack if configured`  
   `→ Audit Service logs completion`

### **`5.2 Long Polling Implementation`**

`go`  
`// Go implementation example (could be Rust, Python, etc.)`  
`type LongPollHandler struct {`  
    `db           *sql.DB`  
    `redis        *redis.Client`  
    `timeout      time.Duration`  
    `maxClients   int`  
    `clientMutex  sync.RWMutex`  
    `clients      map[string][]*ClientConnection`  
`}`

`func (h *LongPollHandler) WaitForCommand(w http.ResponseWriter, r *http.Request) {`  
    `deploymentHash := chi.URLParam(r, "deployment_hash")`  
    `agentToken := r.Header.Get("Authorization")`  
      
    `// Validate agent`  
    `agent, err := h.validateAgent(deploymentHash, agentToken)`  
    `if err != nil {`  
        `http.Error(w, "Unauthorized", http.StatusUnauthorized)`  
        `return`  
    `}`  
      
    `// Set long polling headers`  
    `w.Header().Set("Content-Type", "application/json")`  
    `w.Header().Set("Cache-Control", "no-cache")`  
    `w.Header().Set("Connection", "keep-alive")`  
      
    `// Check for immediate command`  
    `cmd, err := h.getNextCommand(deploymentHash)`  
    `if err == nil && cmd != nil {`  
        `json.NewEncoder(w).Encode(cmd)`  
        `return`  
    `}`  
      
    `// No command, wait for one`  
    `ctx := r.Context()`  
    `timeout := h.getTimeoutParam(r)`  
      
    `select {`  
    `case <-time.After(timeout):`  
        `// Timeout - return 204`  
        `w.WriteHeader(http.StatusNoContent)`  
          
    `case cmd := <-h.waitForCommandSignal(deploymentHash):`  
        `// Command arrived`  
        `json.NewEncoder(w).Encode(cmd)`  
          
    `case <-ctx.Done():`  
        `// Client disconnected`  
        `return`  
    `}`  
`}`

`func (h *LongPollHandler) waitForCommandSignal(deploymentHash string) <-chan *Command {`  
    `ch := make(chan *Command, 1)`  
      
    `h.clientMutex.Lock()`  
    `h.clients[deploymentHash] = append(h.clients[deploymentHash], &ClientConnection{`  
        `Channel: ch,`  
        `Created: time.Now(),`  
    `})`  
    `h.clientMutex.Unlock()`  
      
    `return ch`  
`}`

### **`5.3 WebSocket Gateway Implementation`**

`python`  
`# Python with FastAPI/WebSockets`  
`class WebSocketManager:`  
    `def __init__(self):`  
        `self.active_connections: Dict[str, Dict[str, WebSocket]] = {`  
            `'users': {},`  
            `'agents': {}`  
        `}`  
        `self.connection_locks: Dict[str, asyncio.Lock] = {}`  
      
    `async def connect_agent(self, websocket: WebSocket, deployment_hash: str):`  
        `await websocket.accept()`  
        `self.active_connections['agents'][deployment_hash] = websocket`  
          
        `try:`  
            `while True:`  
                `# Heartbeat handling`  
                `message = await websocket.receive_json()`  
                `if message['type'] == 'heartbeat':`  
                    `await self.handle_agent_heartbeat(deployment_hash, message)`  
                `elif message['type'] == 'log_entry':`  
                    `await self.broadcast_to_user(deployment_hash, message)`  
                `elif message['type'] == 'command_progress':`  
                    `await self.update_command_progress(deployment_hash, message)`  
                      
        `except WebSocketDisconnect:`  
            `self.disconnect_agent(deployment_hash)`  
      
    `async def connect_user(self, websocket: WebSocket, user_id: str):`  
        `await websocket.accept()`  
        `self.active_connections['users'][user_id] = websocket`  
          
        `# Send initial state`  
        `deployments = await self.get_user_deployments(user_id)`  
        `await websocket.send_json({`  
            `'type': 'initial_state',`  
            `'deployments': deployments`  
        `})`  
      
    `async def broadcast_to_user(self, deployment_hash: str, message: dict):`  
        `"""Send agent events to the owning user"""`  
        `user_id = await self.get_user_for_deployment(deployment_hash)`  
        `if user_id in self.active_connections['users']:`  
            `await self.active_connections['users'][user_id].send_json(message)`

## **`6. Multi-Tenant Isolation`**

### **`6.1 Tenant Data Separation`**

`go`  
`// Middleware for tenant isolation`  
`func TenantMiddleware(next http.Handler) http.Handler {`  
    `return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {`  
        `// Extract tenant from JWT or subdomain`  
        `tenantID := extractTenantID(r)`  
          
        `// Add to context`  
        `ctx := context.WithValue(r.Context(), "tenant_id", tenantID)`  
          
        `// Set database schema/connection for tenant`  
        `dbConn := getTenantDBConnection(tenantID)`  
        `ctx = context.WithValue(ctx, "db_conn", dbConn)`  
          
        `next.ServeHTTP(w, r.WithContext(ctx))`  
    `})`  
`}`

`// Row Level Security in PostgreSQL`  
`CREATE POLICY tenant_isolation_policy ON commands`  
    `USING (tenant_id = current_setting('app.current_tenant_id'));`  
      
`ALTER TABLE commands ENABLE ROW LEVEL SECURITY;`

### **`6.2 Resource Quotas per Tenant`**

`yaml`  
`# Tenant quota configuration`  
`tenant_quotas:`  
  `basic:`  
    `max_agents: 10`  
    `max_deployments: 5`  
    `command_rate_limit: 60/hour`  
    `storage_gb: 50`  
    `retention_days: 30`  
      
  `professional:`  
    `max_agents: 100`  
    `max_deployments: 50`  
    `command_rate_limit: 600/hour`  
    `storage_gb: 500`  
    `retention_days: 90`  
      
  `enterprise:`  
    `max_agents: 1000`  
    `max_deployments: 500`  
    `command_rate_limit: 6000/hour`  
    `storage_gb: 5000`  
    `retention_days: 365`

## **`7. Security Requirements`**

### **`7.1 Authentication & Authorization`**

`typescript`  
`// JWT Token Structure`  
`interface AgentToken {`  
  `sub: string;          // agent_id`  
  `deployment_hash: string;`  
  `tenant_id: string;`  
  `capabilities: string[];`  
  `iat: number;          // issued at`  
  `exp: number;          // expiration`  
`}`

`interface UserToken {`  
  `sub: string;          // user_id`  
  `tenant_id: string;`  
  `roles: string[];`  
  `permissions: string[];`  
  `iat: number;`  
  `exp: number;`  
`}`

`// Permission Matrix`  
`const PERMISSIONS = {`  
  `DEPLOYMENT_READ: 'deployment:read',`  
  `DEPLOYMENT_WRITE: 'deployment:write',`  
  `COMMAND_EXECUTE: 'command:execute',`  
  `METRICS_READ: 'metrics:read',`  
  `SETTINGS_MANAGE: 'settings:manage',`  
  `USER_MANAGE: 'user:manage',`  
`};`

`// Role Definitions`  
`const ROLES = {`  
  `ADMIN: [PERMISSIONS.DEPLOYMENT_READ, PERMISSIONS.DEPLOYMENT_WRITE, ...],`  
  `OPERATOR: [PERMISSIONS.DEPLOYMENT_READ, PERMISSIONS.COMMAND_EXECUTE, ...],`  
  `VIEWER: [PERMISSIONS.DEPLOYMENT_READ, PERMISSIONS.METRICS_READ],`  
`};`

### **`7.2 API Security Measures`**

1. **`Rate Limiting`**`:`  
   `go`

`// Redis-based rate limiting`  
`func RateLimitMiddleware(limit int, window time.Duration) gin.HandlerFunc {`  
    `return func(c *gin.Context) {`  
        `key := fmt.Sprintf("rate_limit:%s:%s",`   
            `c.ClientIP(),`   
            `c.Request.URL.Path)`  
          
        `count, _ := redisClient.Incr(key).Result()`  
        `if count == 1 {`  
            `redisClient.Expire(key, window)`  
        `}`  
          
        `if count > int64(limit) {`  
            `c.AbortWithStatusJSON(429, gin.H{"error": "Rate limit exceeded"})`  
            `return`  
        `}`  
          
        `c.Next()`  
    `}`  
`}`

**`Input Validation`**`:`

`python`  
`# Pydantic models for validation`  
`class CommandCreate(BaseModel):`  
    `type: CommandType`  
    `parameters: dict`  
    `priority: Literal["low", "normal", "high", "critical"] = "normal"`  
    `schedule_at: Optional[datetime] = None`  
    `requires_confirmation: bool = False`  
      
    `@validator('parameters')`  
    `def validate_parameters(cls, v, values):`  
        `command_type = values.get('type')`  
        `return CommandValidator.validate(command_type, v)`

**`Agent Authentication`**`:`

`go`  
`// Public key cryptography for agent auth`  
`func VerifyAgentSignature(publicKey string, message []byte, signature []byte) bool {`  
    `pubKey, _ := ssh.ParsePublicKey([]byte(publicKey))`  
    `signedData := struct {`  
        `Message   []byte`  
        `Timestamp int64`  
    `}{`  
        `Message:   message,`  
        `Timestamp: time.Now().Unix(),`  
    `}`  
      
    `marshaled, _ := json.Marshal(signedData)`  
    `return pubKey.Verify(marshaled, &ssh.Signature{`  
        `Format: pubKey.Type(),`  
        `Blob:   signature,`  
    `})`  
`}`

## **`8. Monitoring & Observability`**

### **`8.1 Key Metrics to Monitor`**

`prometheus`  
`# Agent Metrics`  
`trydirect_agents_online{tenant="xyz"}`  
`trydirect_agents_total{tenant="xyz"}`  
`trydirect_agent_heartbeat_latency_seconds{agent="abc123"}`

`# Command Metrics`  
`trydirect_commands_total{type="backup", status="completed"}`  
`trydirect_commands_duration_seconds{type="backup"}`  
`trydirect_commands_queue_size`  
`trydirect_commands_failed_total{error_type="timeout"}`

`# API Metrics`  
`trydirect_api_requests_total{endpoint="/commands", method="POST", status="200"}`  
`trydirect_api_request_duration_seconds{endpoint="/commands"}`  
`trydirect_api_errors_total{type="validation"}`

`# System Metrics`  
`trydirect_database_connections_active`  
`trydirect_redis_memory_usage_bytes`  
`trydirect_queue_processing_lag_seconds`

### **`8.2 Health Check Endpoints`**

`text`  
`GET /health`  
`Response: {`  
  `"status": "healthy",`  
  `"timestamp": "2024-01-15T10:30:00Z",`  
  `"services": {`  
    `"database": "connected",`  
    `"redis": "connected",`  
    `"influxdb": "connected",`  
    `"queue": "processing"`  
  `}`  
`}`

`GET /health/detailed`  
`GET /metrics  # Prometheus metrics`  
`GET /debug/pprof/*  # Go profiling endpoints`

### **`8.3 Alerting Rules`**

`yaml`  
`alerting_rules:`  
  `- alert: HighCommandFailureRate`  
    `expr: rate(trydirect_commands_failed_total[5m]) / rate(trydirect_commands_total[5m]) > 0.1`  
    `for: 5m`  
    `labels:`  
      `severity: warning`  
    `annotations:`  
      `summary: "High command failure rate"`  
      `description: "Command failure rate is {{ $value }} for the last 5 minutes"`  
    
  `- alert: AgentOffline`  
    `expr: time() - trydirect_agent_last_seen_seconds{agent="*"} > 300`  
    `for: 2m`  
    `labels:`  
      `severity: critical`  
    `annotations:`  
      `summary: "Agent {{ $labels.agent }} is offline"`  
    
  `- alert: HighAPILatency`  
    `expr: histogram_quantile(0.95, rate(trydirect_api_request_duration_seconds_bucket[5m])) > 2`  
    `for: 5m`  
    `labels:`  
      `severity: warning`

## **`9. Performance Requirements`**

### **`9.1 Scalability Targets`**

| `Metric` | `Target` | `Notes` |
| ----- | ----- | ----- |
| `Concurrent Agents` | `10,000` | `With connection pooling` |
| `Commands per Second` | `1,000` | `Across all tenants` |
| `WebSocket Connections` | `5,000` | `Per server instance` |
| `Long Polling Connections` | `20,000` | `With efficient timeout handling` |
| `Query Response Time` | `< 100ms` | `95th percentile` |
| `Command Processing Latency` | `< 500ms` | `From queue to agent` |

### **`9.2 Database Performance`**

`sql`  
`-- Required Indexes`  
`CREATE INDEX idx_commands_deployments_status ON commands(deployment_hash, status);`  
`CREATE INDEX idx_commands_created_at ON commands(created_at DESC);`  
`CREATE INDEX idx_command_queue_priority ON command_queue(priority DESC, created_at);`  
`CREATE INDEX idx_agents_last_heartbeat ON agents(last_heartbeat DESC);`  
`CREATE INDEX idx_deployments_tenant ON deployments(tenant_id, created_at);`

`-- Partitioning for large tables`  
`CREATE TABLE commands_2024_01 PARTITION OF commands`  
    `FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');`

### **`9.3 Caching Strategy`**

`go`  
`type CacheManager struct {`  
    `redis *redis.Client`  
    `local *ristretto.Cache // Local in-memory cache`  
`}`

`func (c *CacheManager) GetDeployment(deploymentHash string) (*Deployment, error) {`  
    `// Check local cache first`  
    `if val, ok := c.local.Get(deploymentHash); ok {`  
        `return val.(*Deployment), nil`  
    `}`  
      
    `// Check Redis`  
    `redisKey := fmt.Sprintf("deployment:%s", deploymentHash)`  
    `data, err := c.redis.Get(redisKey).Bytes()`  
    `if err == nil {`  
        `var dep Deployment`  
        `json.Unmarshal(data, &dep)`  
        `c.local.Set(deploymentHash, &dep, 60*time.Second)`  
        `return &dep, nil`  
    `}`  
      
    `// Fall back to database`  
    `dep, err := c.fetchFromDatabase(deploymentHash)`  
    `if err != nil {`  
        `return nil, err`  
    `}`  
      
    `// Cache in both layers`  
    `c.cacheDeployment(dep)`  
    `return dep, nil`  
`}`

## **`10. Deployment Architecture`**

### **`10.1 Kubernetes Deployment`**

`yaml`  
`# deployment.yaml`  
`apiVersion: apps/v1`  
`kind: Deployment`  
`metadata:`  
  `name: trydirect-dashboard`  
`spec:`  
  `replicas: 3`  
  `selector:`  
    `matchLabels:`  
      `app: trydirect-dashboard`  
  `template:`  
    `metadata:`  
      `labels:`  
        `app: trydirect-dashboard`  
    `spec:`  
      `containers:`  
      `- name: api-server`  
        `image: trydirect/dashboard:latest`  
        `ports:`  
        `- containerPort: 5000`  
        `env:`  
        `- name: DATABASE_URL`  
          `valueFrom:`  
            `secretKeyRef:`  
              `name: database-secrets`  
              `key: url`  
        `- name: REDIS_URL`  
          `value: "redis://redis-master:6379"`  
        `resources:`  
          `requests:`  
            `memory: "256Mi"`  
            `cpu: "250m"`  
          `limits:`  
            `memory: "1Gi"`  
            `cpu: "1"`  
        `livenessProbe:`  
          `httpGet:`  
            `path: /health`  
            `port: 5000`  
          `initialDelaySeconds: 30`  
          `periodSeconds: 10`  
        `readinessProbe:`  
          `httpGet:`  
            `path: /health/ready`  
            `port: 5000`  
          `initialDelaySeconds: 5`  
          `periodSeconds: 5`  
`---`  
`# service.yaml`  
`apiVersion: v1`  
`kind: Service`  
`metadata:`  
  `name: trydirect-dashboard`  
`spec:`  
  `selector:`  
    `app: trydirect-dashboard`  
  `ports:`  
  `- port: 80`  
    `targetPort: 5000`  
    `name: http`  
  `- port: 443`  
    `targetPort: 8443`  
    `name: https`  
  `type: LoadBalancer`

### **`10.2 Infrastructure Components`**

`terraform`  
`# Terraform configuration`  
`resource "aws_rds_cluster" "trydirect_db" {`  
  `cluster_identifier = "trydirect-db"`  
  `engine             = "aurora-postgresql"`  
  `engine_version     = "14"`  
  `database_name      = "trydirect"`  
  `master_username    = var.db_username`  
  `master_password    = var.db_password`  
    
  `instance_class     = "db.r6g.large"`  
  `instances = {`  
    `1 = {}`  
    `2 = { promotion_tier = 1 }`  
  `}`  
    
  `backup_retention_period = 30`  
  `preferred_backup_window = "03:00-04:00"`  
`}`

`resource "aws_elasticache_cluster" "trydirect_redis" {`  
  `cluster_id           = "trydirect-redis"`  
  `engine               = "redis"`  
  `node_type            = "cache.r6g.large"`  
  `num_cache_nodes      = 3`  
  `parameter_group_name = "default.redis7"`  
  `port                 = 6379`  
    
  `snapshot_retention_limit = 7`  
  `maintenance_window       = "sun:05:00-sun:09:00"`  
`}`

`resource "aws_influxdb_cluster" "trydirect_metrics" {`  
  `name          = "trydirect-metrics"`  
  `instance_type = "influxdb.r6g.xlarge"`  
  `nodes         = 3`  
    
  `retention_policies = {`  
    `"30d" = 2592000`  
    `"90d" = 7776000`  
    `"1y"  = 31536000`  
  `}`  
`}`

## **`14. Documentation Requirements`**

### **`14.1 API Documentation`**

`yaml`  
`# OpenAPI/Swagger specification`  
`openapi: 3.0.0`  
`info:`  
  `title: Stacker / TryDirect Dashboard API`  
  `version: 1.0.0`  
  `description: |`  
    `API for managing TryDirect Agents and Deployments.`  
      
    `Base URL: https://api.try.direct`  
      
    `Authentication:`  
    `- User API: Bearer token from /auth/login`  
    `- Agent API: Bearer token from /agent/register (GET /wait)`
    `- Stacker → Agent POSTs: HMAC-SHA256 over raw body using agent token`
      `Headers: X-Agent-Id, X-Timestamp, X-Request-Id, X-Agent-Signature`
      `See: STACKER_INTEGRATION_REQUIREMENTS.md`

`paths:`  
  `/api/v1/agent/commands/wait/{deployment_hash}:`  
    `get:`  
      `summary: Wait for next command (Long Polling)`  
      `description: |`  
        `Agents call this endpoint to wait for commands.`  
        `The server will hold the connection open until:`  
        `- A command is available (returns 200)`  
        `- Timeout is reached (returns 204)`  
        `- Connection is closed`  
          
        `Timeout can be specified up to 120 seconds.`  
        
      `parameters:`  
        `- name: deployment_hash`  
          `in: path`  
          `required: true`  
          `schema:`  
            `type: string`  
          `example: "abc123def456"`  
          
        `- name: timeout`  
          `in: query`  
          `schema:`  
            `type: integer`  
            `default: 30`  
            `minimum: 1`  
            `maximum: 120`  
        
      `responses:`  
        `'200':`  
          `description: Command available`  
          `content:`  
            `application/json:`  
              `schema:`  
                `$ref: '#/components/schemas/Command'`  
          
        `'204':`  
          `description: No command available (timeout)`  
          
        `'401':`  
          `description: Unauthorized - invalid or missing token`

### **`14.2 Agent Integration Guide`**

`markdown`  
`# Agent Integration Guide`

`## 1. Registration`  
`` 1. Generate SSH key pair: `ssh-keygen -t ed25519 -f agent_key` ``  
`2. Call registration endpoint with public key`  
`3. Store the returned agent_token securely`

`## 2. Command Polling Loop`  
```` ```python ````  
`while True:`  
    `try:`  
        `command = await long_poll_for_command()`  
        `if command:`  
            `result = await execute_command(command)`  
            `await report_result(command.id, result)`  
    `except Exception as e:`  
        `logger.error(f"Command loop error: {e}")`  
        `await sleep(5)`

## **`3. Real-time Log Streaming`**

`python`  
`async def stream_logs():`  
    `async with websockets.connect(ws_url) as ws:`  
        `while True:`  
            `log_entry = await get_log_entry()`  
            `await ws.send(json.dumps(log_entry))`

## **`4. Health Reporting`**

* `Send heartbeat every 30 seconds via WebSocket`  
* `Report detailed health every 5 minutes via HTTP`  
* `Include system metrics and application status`

`text`  
`## 15. Compliance & Audit`

`### 15.1 Audit Log Requirements`

```` ```go ````  
`type AuditLogger struct {`  
    `db    *sql.DB`  
    `queue chan AuditEvent`  
`}`

`type AuditEvent struct {`  
    `` TenantID     string                 `json:"tenant_id"` ``  
    `` UserID       string                 `json:"user_id"` ``  
    `` Action       string                 `json:"action"` ``  
    `` ResourceType string                 `json:"resource_type"` ``  
    `` ResourceID   string                 `json:"resource_id"` ``  
    `` Details      map[string]interface{} `json:"details"` ``  
    `` IPAddress    string                 `json:"ip_address"` ``  
    `` UserAgent    string                 `json:"user_agent"` ``  
    `` Timestamp    time.Time              `json:"timestamp"` ``  
`}`

`// Actions to audit`  
`var AuditedActions = []string{`  
    `"command.create",`  
    `"command.execute",`  
    `"command.cancel",`  
    `"agent.register",`  
    `"agent.deregister",`  
    `"user.login",`  
    `"user.logout",`  
    `"settings.update",`  
    `"deployment.create",`  
    `"deployment.delete",`  
`}`

### **`15.2 Data Retention Policies`**

`sql`  
`-- Data retention policies`  
`CREATE POLICY command_retention_policy ON commands`  
    `FOR DELETE`  
    `USING (created_at < NOW() - INTERVAL '90 days')`  
    `AND status IN ('completed', 'failed', 'cancelled');`

`CREATE POLICY metrics_retention_policy ON measurements`  
    `FOR DELETE`  
    `USING (time < NOW() - INTERVAL '365 days');`

`-- GDPR compliance: Right to be forgotten`  
`CREATE OR REPLACE FUNCTION delete_user_data(user_id UUID)`  
`RETURNS void AS $$`  
`BEGIN`  
    `-- Anonymize user data`  
    `UPDATE users`   
    `SET email = 'deleted@example.com',`  
        `password_hash = NULL,`  
        `api_key = NULL`  
    `WHERE id = user_id;`  
      
    `-- Delete personal data from logs`  
    `DELETE FROM audit_log`   
    `WHERE user_id = $1;`  
`END;`  
`$$ LANGUAGE plpgsql;`

## 

