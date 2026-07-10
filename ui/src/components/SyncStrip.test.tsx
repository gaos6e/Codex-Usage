import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SyncStrip } from './SyncStrip';

describe('SyncStrip', () => {
  it('shows real import progress and supports cancellation', () => {
    const cancel = vi.fn();
    render(<SyncStrip status={{
      phase: 'importing', filesTotal: 10, filesCompleted: 4,
      bytesTotal: 1000, bytesRead: 500, recordsWritten: 20, recordsSkipped: 0,
      parseFailures: 1, fileErrors: 0, speedBytesPerSecond: 100,
      updatedAtMs: Date.now(), cancelRequested: false,
    }} onCancel={cancel} />);
    expect(screen.getByText(/4\/10 文件/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /取消/ }));
    expect(cancel).toHaveBeenCalledOnce();
  });

  it('labels cancelled imports as resumable', () => {
    render(<SyncStrip status={{
      phase: 'cancelled', filesTotal: 10, filesCompleted: 4,
      bytesTotal: 1000, bytesRead: 500, recordsWritten: 20, recordsSkipped: 0,
      parseFailures: 0, fileErrors: 0, speedBytesPerSecond: 0,
      updatedAtMs: Date.now(), cancelRequested: true,
    }} onCancel={vi.fn()} />);
    expect(screen.getByText('导入已暂停，可继续')).toBeInTheDocument();
  });
});
