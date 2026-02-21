# ADR 007: Agent-driven browser testing for the dashboard

## Status

Accepted

## Context

devrig includes a web-based dashboard for observability (traces, logs,
metrics). This dashboard needs end-to-end (E2E) testing to verify that:

- The UI renders correctly with live data.
- User interactions (filtering, searching, navigating between views) work.
- WebSocket connections for live updates function properly.

Common approaches to E2E testing include:

- **Headless browser automation** (Playwright, Cypress): scripts drive a real
  browser. Reliable but tests are brittle when UI structure changes.
- **Component testing with mocked data**: fast but does not test the full
  stack.
- **Agent-driven testing**: an AI agent interacts with the running application
  through a browser, verifying behavior by observing the rendered page rather
  than relying on CSS selectors or DOM structure.

## Decision

Use agent-driven browser testing for the devrig dashboard. A test agent
launches a real browser, navigates to the dashboard, and verifies behavior by
describing what it sees rather than asserting on specific DOM elements.

The test harness:

1. Starts devrig with a known set of test services.
2. Waits for the dashboard to be reachable.
3. Directs the agent to perform user flows (e.g. "open the traces view and
   verify that spans from the test-api service are visible").
4. The agent reports pass/fail based on what it observes on-screen.

## Consequences

**Positive:**

- Tests real user flows end-to-end, including the backend, WebSocket
  transport, and rendered UI.
- More resilient to UI refactors than selector-based tests: the agent
  verifies semantic content ("I see a span named test-api") rather than DOM
  structure.
- Exercises the same path a real user would take.

**Negative:**

- Slower than unit or component tests. These tests run as part of the
  integration test suite, not on every commit.
- Requires a browser runtime in the CI environment.
- Agent-based assertions may have false positives if the agent misinterprets
  the page. Mitigated by using specific, verifiable assertions.

**Neutral:**

- Traditional Playwright tests can coexist for critical-path smoke tests
  where deterministic assertions are preferred.
