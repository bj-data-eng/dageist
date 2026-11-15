/**
 * @file How a comparison is written wherever the editor shows one. The model
 * and the Polars text of a step keep the comparison as it is typed in code.
 */

import type { Comparison } from './model/source';

/**
 * How each comparison is written in a step. Two-character comparisons that
 * have a sign of their own are written as that sign, so they stay one
 * character wide and read as they do on paper.
 */
export const COMPARISON_SIGNS: Readonly<Record<Comparison, string>> = {
  '==': '==',
  '!=': '≠',
  '>': '>',
  '>=': '≥',
  '<': '<',
  '<=': '≤',
  is_in: 'is_in',
};
