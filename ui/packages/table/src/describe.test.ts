import { describe, expect, it } from 'vitest';

import { describeBlock, describeShape } from './describe';

describe('the line about an opened table', () => {
  it('puts the host figures as they are around where the rows are held', () => {
    expect(
      describeShape(
        { rows: '2,000,066 rows', columns: '300 columns', opened: '10.4 ms' },
        'read from Parquet as you scroll',
      ),
    ).toBe('2,000,066 rows × 300 columns · read from Parquet as you scroll · opened in 10.4 ms');
  });

  it('writes no figure of its own, so a figure it is given is the figure shown', () => {
    expect(describeShape({ rows: 'a', columns: 'b', opened: 'c' }, 'd')).toBe(
      'a × b · d · opened in c',
    );
  });
});

describe('the line about the last block', () => {
  it('follows the host report with how long the view waited, to a tenth of a millisecond', () => {
    expect(describeBlock('140 rows × 17 columns · 38.2 KB · built in 6.2 ms', 31.46)).toBe(
      '140 rows × 17 columns · 38.2 KB · built in 6.2 ms · fetched and decoded in 31.5 ms',
    );
  });

  it('keeps the report as it is, whatever it says', () => {
    expect(describeBlock('anything', 0)).toBe('anything · fetched and decoded in 0.0 ms');
  });
});
