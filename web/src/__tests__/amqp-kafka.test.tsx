import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { STEP_TYPES } from '../api';
import { STARTER_TEMPLATES } from '../templates';
import StepConfigPanel from '../components/StepConfigPanel';

describe('AMQP & Kafka source step types', () => {
  // ── API layer ──
  it('STEP_TYPES includes amqp_source', () => {
    expect(STEP_TYPES).toContain('amqp_source');
  });

  it('STEP_TYPES includes kafka_source', () => {
    expect(STEP_TYPES).toContain('kafka_source');
  });

  // ── Config panel structured fields ──
  it('amqp_source config shows amqp_url and queue fields', () => {
    render(
      <StepConfigPanel
        stepId="s1"
        stepName="RabbitMQ"
        stepType="amqp_source"
        config={{}}
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(/AMQP URL/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Queue/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Exchange/i)).toBeInTheDocument();
  });

  it('kafka_source config shows brokers and topic fields', () => {
    render(
      <StepConfigPanel
        stepId="s2"
        stepName="Kafka"
        stepType="kafka_source"
        config={{}}
        onUpdate={vi.fn()}
        onDelete={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(/Brokers/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Topic/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/Group ID/i)).toBeInTheDocument();
  });
});
