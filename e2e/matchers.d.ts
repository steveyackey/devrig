import 'vitest';

interface PlaywrightMatchers<R = unknown> {
  toBeVisible(options?: { timeout?: number }): Promise<R>;
  toBeHidden(options?: { timeout?: number }): Promise<R>;
  toHaveText(expected: string | RegExp, options?: { timeout?: number }): Promise<R>;
  toHaveAttribute(
    name: string,
    value: string | RegExp,
    options?: { timeout?: number },
  ): Promise<R>;
  toHaveClass(expected: string | RegExp, options?: { timeout?: number }): Promise<R>;
  toHaveCount(expected: number, options?: { timeout?: number }): Promise<R>;
  toHaveValue(expected: string, options?: { timeout?: number }): Promise<R>;
  toBeDisabled(options?: { timeout?: number }): Promise<R>;
  toBeEnabled(options?: { timeout?: number }): Promise<R>;
}

declare module 'vitest' {
  interface Assertion<T = any> extends PlaywrightMatchers<void> {}
  interface AsymmetricMatchersContaining extends PlaywrightMatchers<void> {}
}
