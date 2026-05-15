import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';

import StepConfigPanel from '../components/StepConfigPanel';

const defaultProps = {
  stepId: 's1',
  stepName: 'My Source',
  stepType: 'source',
  config: {},
  onUpdate: vi.fn(),
  onDelete: vi.fn(),
  onClose: vi.fn(),
};

beforeEach(() => {
  vi.clearAllMocks();
});

describe('StepConfigPanel — structured config forms', () => {
  it('shows endpoint URL field for source step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="source" />);
    expect(screen.getByLabelText(/endpoint url/i)).toBeInTheDocument();
  });

  it('shows method selector for source step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="source" />);
    expect(screen.getByLabelText(/method/i)).toBeInTheDocument();
  });

  it('shows field/operator/value for condition step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="condition" />);
    expect(screen.getByLabelText(/field/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/operator/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/value/i)).toBeInTheDocument();
  });

  it('shows ws_url for ws_source step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="ws_source" />);
    expect(screen.getByLabelText(/websocket url/i)).toBeInTheDocument();
  });

  it('shows host/port/service for grpc_source step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="grpc_source" />);
    expect(screen.getByLabelText(/host/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/port/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/service/i)).toBeInTheDocument();
  });

  it('shows connection_string/table for cdc_source step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="cdc_source" />);
    expect(screen.getByLabelText(/connection string/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/table/i)).toBeInTheDocument();
  });

  it('shows strategy for parallel_split step type', () => {
    render(<StepConfigPanel {...defaultProps} stepType="parallel_split" />);
    expect(screen.getByLabelText(/strategy/i)).toBeInTheDocument();
  });

  it('has Advanced JSON toggle that shows raw textarea', () => {
    render(<StepConfigPanel {...defaultProps} stepType="source" />);
    const toggle = screen.getByText(/advanced json/i);
    expect(toggle).toBeInTheDocument();
    fireEvent.click(toggle);
    expect(screen.getByRole('textbox', { name: /json/i }) || screen.getByDisplayValue(/{}/)).toBeTruthy();
  });

  it('saves structured form data correctly via onUpdate', () => {
    const onUpdate = vi.fn();
    render(<StepConfigPanel {...defaultProps} stepType="source" config={{ endpoint_url: 'http://old' }} onUpdate={onUpdate} />);

    const urlInput = screen.getByLabelText(/endpoint url/i);
    fireEvent.change(urlInput, { target: { value: 'http://new-endpoint' } });

    fireEvent.click(screen.getByText('Save'));

    expect(onUpdate).toHaveBeenCalledWith('s1', 'My Source', expect.objectContaining({ endpoint_url: 'http://new-endpoint' }));
  });

  it('pre-fills form fields from existing config', () => {
    render(<StepConfigPanel {...defaultProps} stepType="source" config={{ endpoint_url: 'http://example.com', method: 'POST' }} />);

    const urlInput = screen.getByLabelText(/endpoint url/i) as HTMLInputElement;
    expect(urlInput.value).toBe('http://example.com');
  });

  it('Type field is clearly read-only with label', () => {
    render(<StepConfigPanel {...defaultProps} stepType="source" />);
    // Type should not be an input field — it should be visually distinct
    const typeDisplay = screen.getByText('source');
    expect(typeDisplay).toBeInTheDocument();
    // Should not be an <input> element
    expect(typeDisplay.tagName).not.toBe('INPUT');
  });
});
