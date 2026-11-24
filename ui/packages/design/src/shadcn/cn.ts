/**
 * @file shadcn/ui's class-name helper: joins class names like `cx`, then lets
 * a later Tailwind class win over an earlier one it conflicts with, so a
 * component's caller can override its classes.
 */

import { clsx } from 'clsx';
import type { ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

/** Joins `inputs` into one class string, the later of two conflicting classes winning. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
