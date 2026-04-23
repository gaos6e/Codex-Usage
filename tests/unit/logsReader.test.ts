import { describe, expect, it } from 'vitest';
import { extractBreakdownFromText } from '../../src/main/services/logsReader';

describe('logs token breakdown parsing', () => {
  it('prefers response usage over earlier tool usage fields', () => {
    const body = [
      'session_loop: websocket event: ',
      JSON.stringify({
        type: 'response.completed',
        tool_usage: {
          image_gen: {
            input_tokens: 0,
            output_tokens: 0,
          },
        },
        response: {
          usage: {
            input_tokens: 28370,
            input_tokens_details: {
              cached_tokens: 2432,
            },
            output_tokens: 91,
            output_tokens_details: {
              reasoning_tokens: 53,
            },
            total_tokens: 28461,
          },
        },
      }),
    ].join('');

    expect(extractBreakdownFromText(body)).toMatchObject({
      input: 28370,
      cached: 2432,
      output: 91,
      reasoning: 53,
      cacheHitRate: 2432 / 28370,
    });
  });

  it('keeps zero cache hits as a valid 0% rate', () => {
    const body = JSON.stringify({
      response: {
        usage: {
          input_tokens: 15434,
          input_tokens_details: {
            cached_tokens: 0,
          },
          output_tokens: 144,
          output_tokens_details: {
            reasoning_tokens: 124,
          },
        },
      },
    });

    expect(extractBreakdownFromText(body)).toMatchObject({
      input: 15434,
      cached: 0,
      cacheHitRate: 0,
    });
  });

  it('falls back to scanning all textual token fields', () => {
    const body = 'tool input_tokens: 0 response input_tokens: 100 cached_tokens: 25 output_tokens: 5';
    expect(extractBreakdownFromText(body)).toMatchObject({
      input: 100,
      cached: 25,
      output: 5,
      cacheHitRate: 0.25,
    });
  });
});
