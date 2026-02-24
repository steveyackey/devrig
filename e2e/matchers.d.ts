declare module 'bun:test' {
  interface Matchers<T> {
    toBeVisible(options?: { timeout?: number }): Promise<void>;
    toBeHidden(options?: { timeout?: number }): Promise<void>;
    toHaveText(expected: string | RegExp, options?: { timeout?: number }): Promise<void>;
    toHaveAttribute(
      name: string,
      value: string | RegExp,
      options?: { timeout?: number },
    ): Promise<void>;
    toHaveClass(expected: string | RegExp, options?: { timeout?: number }): Promise<void>;
    toHaveCount(expected: number, options?: { timeout?: number }): Promise<void>;
    toHaveValue(expected: string, options?: { timeout?: number }): Promise<void>;
    toBeDisabled(options?: { timeout?: number }): Promise<void>;
    toBeEnabled(options?: { timeout?: number }): Promise<void>;
  }
}
