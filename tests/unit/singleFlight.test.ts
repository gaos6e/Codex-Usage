import { describe, expect, it } from 'vitest';
import { SingleFlight } from '../../src/main/services/singleFlight';

describe('SingleFlight', () => {
  it('shares an in-flight task for the same key', async () => {
    const gate = new SingleFlight();
    let calls = 0;
    let release: (() => void) | undefined;
    const blocker = new Promise<void>((resolve) => {
      release = resolve;
    });

    const first = gate.run('refresh', async () => {
      calls += 1;
      await blocker;
      return 42;
    });
    const second = gate.run('refresh', async () => {
      calls += 1;
      return 7;
    });

    expect(second).toBe(first);
    release?.();
    await expect(second).resolves.toBe(42);
    expect(calls).toBe(1);
  });

  it('allows a new task after the previous task settles', async () => {
    const gate = new SingleFlight();
    const first = await gate.run('refresh', async () => 1);
    const second = await gate.run('refresh', async () => 2);

    expect(first).toBe(1);
    expect(second).toBe(2);
  });
});
