import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import React from 'react';

import { ToastProvider, useToast, ToastContainer } from '../components/Toast';

// Helper component that exposes useToast for testing
function ToastTrigger({ type, message }: { type: 'success' | 'error' | 'info'; message: string }) {
  const toast = useToast();
  return (
    <button onClick={() => toast[type](message)}>
      trigger-{type}
    </button>
  );
}

describe('Toast notification system', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders success toast when triggered', () => {
    render(
      <ToastProvider>
        <ToastTrigger type="success" message="Step saved!" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByText('trigger-success'));
    expect(screen.getByText('Step saved!')).toBeInTheDocument();
  });

  it('renders error toast when triggered', () => {
    render(
      <ToastProvider>
        <ToastTrigger type="error" message="API 401: Unauthorized" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByText('trigger-error'));
    expect(screen.getByText('API 401: Unauthorized')).toBeInTheDocument();
  });

  it('auto-dismisses success toast after 4 seconds', () => {
    render(
      <ToastProvider>
        <ToastTrigger type="success" message="Done!" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByText('trigger-success'));
    expect(screen.getByText('Done!')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(4100);
    });

    expect(screen.queryByText('Done!')).not.toBeInTheDocument();
  });

  it('error toast stays visible until manually dismissed', () => {
    render(
      <ToastProvider>
        <ToastTrigger type="error" message="Failed!" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByText('trigger-error'));
    expect(screen.getByText('Failed!')).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(screen.getByText('Failed!')).toBeInTheDocument();
  });

  it('can show multiple toasts simultaneously', () => {
    render(
      <ToastProvider>
        <ToastTrigger type="success" message="First" />
        <ToastTrigger type="error" message="Second" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByText('trigger-success'));
    fireEvent.click(screen.getByText('trigger-error'));

    expect(screen.getByText('First')).toBeInTheDocument();
    expect(screen.getByText('Second')).toBeInTheDocument();
  });

  it('dismiss button removes error toast', () => {
    render(
      <ToastProvider>
        <ToastTrigger type="error" message="Dismiss me" />
      </ToastProvider>,
    );

    fireEvent.click(screen.getByText('trigger-error'));
    expect(screen.getByText('Dismiss me')).toBeInTheDocument();

    const dismissBtn = screen.getByRole('button', { name: '×' });
    fireEvent.click(dismissBtn);

    expect(screen.queryByText('Dismiss me')).not.toBeInTheDocument();
  });
});
