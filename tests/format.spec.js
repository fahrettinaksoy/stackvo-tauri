import { describe, it, expect } from 'vitest';
import { bytes, bytesPerSecond, percent, duration, loadColor } from '@/lib/format';

/**
 * Every one of these functions is fed values that come from Docker or from the
 * host sampler, which means every one of them will eventually be handed
 * `undefined` — a container that stopped between the list call and the stats
 * call, a metric the platform does not expose. The dash is the contract: a
 * missing reading is shown as missing, never as zero, because "0 B" and "we
 * could not measure" mean different things to someone debugging a stack.
 */

describe('bytes', () => {
  it('shows a dash rather than inventing a zero', () => {
    expect(bytes(null)).toBe('—');
    expect(bytes(undefined)).toBe('—');
    expect(bytes(NaN)).toBe('—');
  });

  it('distinguishes a measured zero from a missing reading', () => {
    expect(bytes(0)).toBe('0 B');
  });

  it('drops the decimal on whole bytes but keeps it above', () => {
    expect(bytes(512)).toBe('512 B');
    expect(bytes(1024)).toBe('1.0 KB');
  });

  it('scales through the unit table', () => {
    expect(bytes(1024 ** 2)).toBe('1.0 MB');
    expect(bytes(1024 ** 3)).toBe('1.0 GB');
    expect(bytes(1024 ** 4)).toBe('1.0 TB');
  });

  it('clamps at the largest unit instead of running off the end', () => {
    // Beyond petabytes the exponent would index past UNITS and print
    // "undefined" next to a number.
    expect(bytes(1024 ** 7)).toMatch(/PB$/);
  });

  it('honours the requested precision', () => {
    expect(bytes(1536, 2)).toBe('1.50 KB');
  });
});

describe('bytesPerSecond', () => {
  it('carries the dash through rather than printing "—/s"', () => {
    expect(bytesPerSecond(null)).toBe('—');
  });

  it('suffixes a real rate', () => {
    expect(bytesPerSecond(2048)).toBe('2.0 KB/s');
  });
});

describe('percent', () => {
  it('shows a dash for a missing reading', () => {
    expect(percent(undefined)).toBe('—');
    expect(percent(NaN)).toBe('—');
  });

  it('formats a measured value', () => {
    expect(percent(0)).toBe('0.0%');
    expect(percent(12.345)).toBe('12.3%');
    expect(percent(12.345, 0)).toBe('12%');
  });

  it('does not clamp above 100 — a multi-core container really can exceed it', () => {
    expect(percent(180.5)).toBe('180.5%');
  });
});

describe('duration', () => {
  it('shows a dash for a missing reading', () => {
    expect(duration(null)).toBe('—');
  });

  it('compacts to the two largest units', () => {
    // Minutes are the smallest unit by design — a container that started
    // seconds ago reads as "0m", not as a seconds counter that churns.
    expect(duration(45)).toBe('0m');
    expect(duration(12 * 60)).toBe('12m');
    expect(duration(4 * 3600 + 12 * 60)).toBe('4h 12m');
    expect(duration(3 * 86400 + 4 * 3600)).toBe('3d 4h');
  });
});

describe('loadColor', () => {
  it('escalates at the documented thresholds', () => {
    expect(loadColor(10)).toBe('success');
    expect(loadColor(75)).toBe('warning');
    expect(loadColor(95)).toBe('error');
  });

  it('accepts caller-supplied thresholds', () => {
    expect(loadColor(50, { warn: 40, danger: 60 })).toBe('warning');
    expect(loadColor(70, { warn: 40, danger: 60 })).toBe('error');
  });
});
