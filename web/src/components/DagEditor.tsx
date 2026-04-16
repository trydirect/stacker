import React, { useCallback, useState, useEffect, useRef } from 'react';
import {
  ReactFlow,
  addEdge,
  useNodesState,
  useEdgesState,
  Controls,
  MiniMap,
  Background,
  BackgroundVariant,
  type Connection,
  type Edge,
  type Node,
  type ReactFlowInstance,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import DagNode, { type DagNodeData } from './DagNode';
import StepPalette from './StepPalette';
import StepConfigPanel from './StepConfigPanel';
import TemplatePicker from './TemplatePicker';
import Toolbar from './Toolbar';
import { useToast } from './Toast';
import { api, type StepType, type DagStep, type DagEdge as ApiEdge } from '../api';
import { stepColor } from '../theme';
import { type StarterTemplate } from '../templates';

const nodeTypes = { dag: DagNode };

interface DagEditorProps {
  templateId: string;
  instanceId?: string;
  token: string;
}

let stepCounter = 0;

const DagEditor: React.FC<DagEditorProps> = ({ templateId, instanceId, token }) => {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [selectedStep, setSelectedStep] = useState<string | null>(null);
  const [validationResult, setValidationResult] = useState<{ valid: boolean; errors?: string[] } | null>(null);
  const [executing, setExecuting] = useState(false);
  const reactFlowWrapper = useRef<HTMLDivElement>(null);
  const [reactFlowInstance, setReactFlowInstance] = useState<ReactFlowInstance | null>(null);
  const toast = useToast();

  const isDemo = !token || token === 'dev-token';
  const [showTemplatePicker, setShowTemplatePicker] = useState(isDemo);

  const onSelectTemplate = useCallback((tpl: StarterTemplate | null) => {
    setShowTemplatePicker(false);
    if (!tpl) return;
    const newNodes = tpl.steps.map((s, i) => ({
      id: s.id,
      type: 'dag' as const,
      position: { x: 100 + (i % 4) * 200, y: 80 + Math.floor(i / 4) * 120 },
      data: { label: s.name, stepType: s.step_type, config: s.config } satisfies DagNodeData,
    }));
    const newEdges = tpl.edges.map((e, i) => ({
      id: `tpl-edge-${i}`,
      source: e.source,
      target: e.target,
      animated: true,
      style: { stroke: '#888' },
    }));
    setNodes(newNodes);
    setEdges(newEdges);
    toast.success(`Loaded template: ${tpl.name}`);
  }, [setNodes, setEdges, toast]);

  // Load existing DAG (skip in demo mode)
  useEffect(() => {
    if (isDemo) return;
    const load = async () => {
      try {
        const [steps, apiEdges] = await Promise.all([
          api.listSteps(templateId, token),
          api.listEdges(templateId, token),
        ]);
        setNodes(stepsToNodes(steps));
        setEdges(apiEdgesToFlowEdges(apiEdges));
      } catch {
        // New template — start empty
      }
    };
    load();
  }, [templateId, token, setNodes, setEdges]);

  const onConnect = useCallback(
    (params: Connection) => {
      setEdges((eds) => addEdge({ ...params, animated: true, style: { stroke: '#666' } }, eds));
      if (!isDemo && params.source && params.target) {
        api.addEdge(templateId, { from_step_id: params.source, to_step_id: params.target }, token).catch((e: Error) => toast.error(e.message));
      }
    },
    [setEdges, templateId, token, isDemo],
  );

  const onAddStep = useCallback(
    (stepType: StepType, name: string) => {
      stepCounter++;
      const id = `new-${stepCounter}-${Date.now()}`;
      const position = { x: 100 + (stepCounter % 4) * 200, y: 80 + Math.floor(stepCounter / 4) * 120 };
      const newNode: Node = {
        id,
        type: 'dag',
        position,
        data: { label: name || stepType, stepType, config: {} } satisfies DagNodeData,
      };
      setNodes((nds) => [...nds, newNode]);

      if (!isDemo) {
        api
          .addStep(templateId, { name: name || stepType, step_type: stepType, config: {} }, token)
          .then((created) => {
            setNodes((nds) =>
              nds.map((n) => (n.id === id ? { ...n, id: created.id } : n)),
            );
            setEdges((eds) =>
              eds.map((e) => ({
                ...e,
                source: e.source === id ? created.id : e.source,
                target: e.target === id ? created.id : e.target,
              })),
            );
          })
          .catch((e: Error) => toast.error(e.message));
      }
    },
    [setNodes, setEdges, templateId, token, isDemo],
  );

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();
      const stepType = event.dataTransfer.getData('stepType') as StepType;
      if (!stepType || !reactFlowInstance || !reactFlowWrapper.current) return;

      const bounds = reactFlowWrapper.current.getBoundingClientRect();
      const position = reactFlowInstance.screenToFlowPosition({
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });

      stepCounter++;
      const id = `new-${stepCounter}-${Date.now()}`;
      const newNode: Node = {
        id,
        type: 'dag',
        position,
        data: { label: stepType.replace(/_/g, ' '), stepType, config: {} } satisfies DagNodeData,
      };
      setNodes((nds) => [...nds, newNode]);

      if (!isDemo) {
        api
          .addStep(templateId, { name: stepType, step_type: stepType, config: {} }, token)
          .then((created) => {
            setNodes((nds) => nds.map((n) => (n.id === id ? { ...n, id: created.id } : n)));
          })
          .catch((e: Error) => toast.error(e.message));
      }
    },
    [reactFlowInstance, setNodes, templateId, token, isDemo],
  );

  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  const onNodeClick = useCallback((_: React.MouseEvent, node: Node) => {
    setSelectedStep(node.id);
  }, []);

  const onUpdateStep = useCallback(
    (stepId: string, name: string, config: Record<string, unknown>) => {
      setNodes((nds) =>
        nds.map((n) =>
          n.id === stepId ? { ...n, data: { ...n.data, label: name, config } } : n,
        ),
      );
      if (!isDemo) {
        api.updateStep(templateId, stepId, { name, config }, token).catch((e: Error) => toast.error(e.message));
      }
    },
    [setNodes, templateId, token, isDemo],
  );

  const onDeleteStep = useCallback(
    (stepId: string) => {
      setNodes((nds) => nds.filter((n) => n.id !== stepId));
      setEdges((eds) => eds.filter((e) => e.source !== stepId && e.target !== stepId));
      setSelectedStep(null);
      if (!isDemo) {
        api.deleteStep(templateId, stepId, token).catch((e: Error) => toast.error(e.message));
      }
    },
    [setNodes, setEdges, templateId, token, isDemo],
  );

  const onEdgesDelete = useCallback(
    (deletedEdges: Edge[]) => {
      if (!isDemo) {
        for (const edge of deletedEdges) {
          api.deleteEdge(templateId, edge.id, token).catch((e: Error) => toast.error(e.message));
        }
      }
    },
    [templateId, token, isDemo],
  );

  const onValidate = useCallback(async () => {
    if (isDemo) {
      // Local validation in demo mode
      const errors: string[] = [];
      if (nodes.length === 0) errors.push('DAG has no steps');
      const result = { valid: errors.length === 0, errors };
      setValidationResult(result);
      if (result.valid) toast.success('DAG is valid (demo)');
      else toast.error(`Validation: ${errors.join(', ')}`);
      return;
    }
    try {
      const result = await api.validateDag(templateId, token);
      setValidationResult(result);
      if (result.valid) toast.success('DAG is valid');
      else toast.error(`Validation failed: ${result.errors?.join(', ')}`);
    } catch (e) {
      const msg = (e as Error).message;
      setValidationResult({ valid: false, errors: [msg] });
      toast.error(msg);
    }
  }, [templateId, token, toast, isDemo, nodes]);

  const onExecute = useCallback(async () => {
    if (isDemo) {
      toast.info('Demo mode — execution is not available without authentication');
      return;
    }
    if (!instanceId) return;
    setExecuting(true);
    setValidationResult(null);

    setNodes((nds) =>
      nds.map((n) => ({ ...n, data: { ...n.data, executionStatus: 'pending', error: undefined } })),
    );

    try {
      const result = await api.executeDag(instanceId, {}, token);
      setNodes((nds) =>
        nds.map((n) => {
          const sr = result.step_results.find((r) => r.step_id === n.id);
          return sr
            ? { ...n, data: { ...n.data, executionStatus: sr.status, error: sr.error } }
            : n;
        }),
      );
      toast.success(`Execution complete: ${result.completed_steps}/${result.total_steps} steps`);
    } catch (e) {
      const msg = (e as Error).message;
      setValidationResult({ valid: false, errors: [msg] });
      toast.error(msg);
    } finally {
      setExecuting(false);
    }
  }, [instanceId, templateId, token, setNodes, toast]);

  const selectedNode = nodes.find((n) => n.id === selectedStep);

  return (
    <div ref={reactFlowWrapper} style={{ width: '100%', height: '100vh' }}>
      {isDemo && (
        <div
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            right: 0,
            zIndex: 20,
            background: '#ff9800',
            color: '#fff',
            textAlign: 'center',
            padding: '6px 12px',
            fontSize: 13,
            fontWeight: 600,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 12,
          }}
        >
          <span>Demo mode — changes not saved</span>
          <a
            href="/login"
            style={{
              background: '#fff',
              color: '#ff9800',
              padding: '3px 12px',
              borderRadius: 4,
              fontSize: 12,
              fontWeight: 700,
              textDecoration: 'none',
              whiteSpace: 'nowrap',
            }}
          >
            Sign Up / Login
          </a>
        </div>
      )}
      {showTemplatePicker && <TemplatePicker onSelect={onSelectTemplate} />}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onEdgesDelete={onEdgesDelete}
        onInit={setReactFlowInstance}
        onDrop={onDrop}
        onDragOver={onDragOver}
        onNodeClick={onNodeClick}
        nodeTypes={nodeTypes}
        deleteKeyCode={['Backspace', 'Delete']}
        fitView
        snapToGrid
        snapGrid={[20, 20]}
      >
        <Controls />
        <MiniMap
          nodeColor={(n) => {
            const data = n.data as unknown as DagNodeData;
            return stepColor(data.stepType).border;
          }}
        />
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} />
      </ReactFlow>

      <StepPalette onAddStep={onAddStep} />

      {selectedNode && (
        <StepConfigPanel
          stepId={selectedNode.id}
          stepName={String((selectedNode.data as unknown as DagNodeData).label)}
          stepType={String((selectedNode.data as unknown as DagNodeData).stepType)}
          config={(selectedNode.data as unknown as DagNodeData).config ?? {}}
          onUpdate={onUpdateStep}
          onDelete={onDeleteStep}
          onClose={() => setSelectedStep(null)}
        />
      )}

      <Toolbar
        templateId={templateId}
        onValidate={onValidate}
        onExecute={onExecute}
        validationResult={validationResult}
        executing={executing}
      />
    </div>
  );
};

// Helpers
function stepsToNodes(steps: DagStep[]): Node[] {
  return steps.map((s, i) => ({
    id: s.id,
    type: 'dag',
    position: { x: 100 + (i % 4) * 200, y: 80 + Math.floor(i / 4) * 120 },
    data: { label: s.name, stepType: s.step_type, config: s.config } satisfies DagNodeData,
  }));
}

function apiEdgesToFlowEdges(apiEdges: ApiEdge[]): Edge[] {
  return apiEdges.map((e) => ({
    id: e.id,
    source: e.from_step_id,
    target: e.to_step_id,
    animated: true,
    style: { stroke: '#666' },
  }));
}

export default DagEditor;
