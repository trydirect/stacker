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
import Toolbar from './Toolbar';
import { api, type StepType, type DagStep, type DagEdge as ApiEdge } from '../api';
import { stepColor } from '../theme';

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

  // Load existing DAG
  useEffect(() => {
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
      if (params.source && params.target) {
        api.addEdge(templateId, { from_step_id: params.source, to_step_id: params.target }, token).catch(console.error);
      }
    },
    [setEdges, templateId, token],
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

      api
        .addStep(templateId, { step_name: name || stepType, step_type: stepType, config: {}, position_x: position.x, position_y: position.y }, token)
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
        .catch(console.error);
    },
    [setNodes, setEdges, templateId, token],
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

      api
        .addStep(templateId, { step_name: stepType, step_type: stepType, config: {}, position_x: Math.round(position.x), position_y: Math.round(position.y) }, token)
        .then((created) => {
          setNodes((nds) => nds.map((n) => (n.id === id ? { ...n, id: created.id } : n)));
        })
        .catch(console.error);
    },
    [reactFlowInstance, setNodes, templateId, token],
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
      api.updateStep(templateId, stepId, { step_name: name, config }, token).catch(console.error);
    },
    [setNodes, templateId, token],
  );

  const onDeleteStep = useCallback(
    (stepId: string) => {
      setNodes((nds) => nds.filter((n) => n.id !== stepId));
      setEdges((eds) => eds.filter((e) => e.source !== stepId && e.target !== stepId));
      setSelectedStep(null);
      api.deleteStep(templateId, stepId, token).catch(console.error);
    },
    [setNodes, setEdges, templateId, token],
  );

  const onValidate = useCallback(async () => {
    try {
      const result = await api.validateDag(templateId, token);
      setValidationResult(result);
    } catch (e) {
      setValidationResult({ valid: false, errors: [(e as Error).message] });
    }
  }, [templateId, token]);

  const onExecute = useCallback(async () => {
    if (!instanceId) return;
    setExecuting(true);
    setValidationResult(null);

    // Reset step statuses
    setNodes((nds) =>
      nds.map((n) => ({ ...n, data: { ...n.data, executionStatus: 'pending', error: undefined } })),
    );

    try {
      const result = await api.executeDag(instanceId, {}, token);
      // Update node statuses from results
      setNodes((nds) =>
        nds.map((n) => {
          const sr = result.step_results.find((r) => r.step_id === n.id);
          return sr
            ? { ...n, data: { ...n.data, executionStatus: sr.status, error: sr.error } }
            : n;
        }),
      );
    } catch (e) {
      setValidationResult({ valid: false, errors: [(e as Error).message] });
    } finally {
      setExecuting(false);
    }
  }, [instanceId, templateId, token, setNodes]);

  const selectedNode = nodes.find((n) => n.id === selectedStep);

  return (
    <div ref={reactFlowWrapper} style={{ width: '100%', height: '100vh' }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        onInit={setReactFlowInstance}
        onDrop={onDrop}
        onDragOver={onDragOver}
        onNodeClick={onNodeClick}
        nodeTypes={nodeTypes}
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
    position: { x: s.position_x ?? 100 + (i % 4) * 200, y: s.position_y ?? 80 + Math.floor(i / 4) * 120 },
    data: { label: s.step_name, stepType: s.step_type, config: s.config } satisfies DagNodeData,
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
