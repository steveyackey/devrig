import { expect } from 'bun:test';
import type { Locator } from 'playwright';

/**
 * Poll a condition until it matches the expected state, retrying every 100ms.
 * For normal assertions (isNot=false), polls until condition returns true.
 * For negated assertions (isNot=true), polls until condition returns false.
 */
async function pollUntil(
  fn: () => Promise<boolean>,
  isNot: boolean,
  timeout: number,
): Promise<boolean> {
  const deadline = Date.now() + timeout;
  let last = false;
  do {
    last = await fn();
    if (isNot ? !last : last) return last;
    await Bun.sleep(100);
  } while (Date.now() < deadline);
  return last;
}

const DEFAULT_TIMEOUT = 5000;

expect.extend({
  async toBeVisible(received: Locator, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    const pass = await pollUntil(() => received.isVisible(), this.isNot, timeout);
    return {
      pass,
      message: () => `expected locator ${pass ? '' : 'not '}to be visible`,
    };
  },

  async toBeHidden(received: Locator, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    const pass = await pollUntil(() => received.isHidden(), this.isNot, timeout);
    return {
      pass,
      message: () => `expected locator ${pass ? '' : 'not '}to be hidden`,
    };
  },

  async toHaveText(received: Locator, expected: string | RegExp, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    let lastText = '';
    const pass = await pollUntil(async () => {
      lastText = (await received.textContent()) ?? '';
      if (expected instanceof RegExp) return expected.test(lastText);
      return lastText.trim() === expected;
    }, this.isNot, timeout);
    return {
      pass,
      message: () =>
        `expected locator text ${pass ? '' : 'not '}to match ${expected}, got "${lastText}"`,
    };
  },

  async toHaveAttribute(
    received: Locator,
    name: string,
    value: string | RegExp,
    options?: { timeout?: number },
  ) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    let lastValue = '';
    const pass = await pollUntil(async () => {
      lastValue = (await received.getAttribute(name)) ?? '';
      if (value instanceof RegExp) return value.test(lastValue);
      return lastValue === value;
    }, this.isNot, timeout);
    return {
      pass,
      message: () =>
        `expected attribute "${name}" ${pass ? '' : 'not '}to be ${value}, got "${lastValue}"`,
    };
  },

  async toHaveClass(received: Locator, expected: string | RegExp, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    let lastClass = '';
    const pass = await pollUntil(async () => {
      lastClass = (await received.getAttribute('class')) ?? '';
      if (expected instanceof RegExp) return expected.test(lastClass);
      return lastClass.includes(expected);
    }, this.isNot, timeout);
    return {
      pass,
      message: () =>
        `expected class ${pass ? '' : 'not '}to match ${expected}, got "${lastClass}"`,
    };
  },

  async toHaveCount(received: Locator, expected: number, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    let lastCount = 0;
    const pass = await pollUntil(async () => {
      lastCount = await received.count();
      return lastCount === expected;
    }, this.isNot, timeout);
    return {
      pass,
      message: () =>
        `expected count ${pass ? '' : 'not '}to be ${expected}, got ${lastCount}`,
    };
  },

  async toHaveValue(received: Locator, expected: string, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    let lastValue = '';
    const pass = await pollUntil(async () => {
      lastValue = await received.inputValue();
      return lastValue === expected;
    }, this.isNot, timeout);
    return {
      pass,
      message: () =>
        `expected value ${pass ? '' : 'not '}to be "${expected}", got "${lastValue}"`,
    };
  },

  async toBeDisabled(received: Locator, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    const pass = await pollUntil(() => received.isDisabled(), this.isNot, timeout);
    return {
      pass,
      message: () => `expected locator ${pass ? '' : 'not '}to be disabled`,
    };
  },

  async toBeEnabled(received: Locator, options?: { timeout?: number }) {
    const timeout = options?.timeout ?? DEFAULT_TIMEOUT;
    const pass = await pollUntil(() => received.isEnabled(), this.isNot, timeout);
    return {
      pass,
      message: () => `expected locator ${pass ? '' : 'not '}to be enabled`,
    };
  },
});
