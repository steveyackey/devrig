import { Component, createSignal, createEffect, onCleanup, Show, Switch, Match } from 'solid-js';
import { connectWebSocket, type TelemetryEvent } from './api';
import Sidebar from './components/Sidebar';
import StatusBar from './components/StatusBar';
import CommandPalette from './components/CommandPalette';
import TracesView from './views/TracesView';
import TraceDetail from './views/TraceDetail';
import LogsView from './views/LogsView';
import MetricsView from './views/MetricsView';
import StatusView from './views/StatusView';
import ConfigView from './views/ConfigView';
import { ToastProvider } from './components/ui';
import { initTheme } from './lib/theme';

const App: Component = () => {
  initTheme();
  const [route, setRoute] = createSignal(getHashRoute());
  const [latestEvent, setLatestEvent] = createSignal<TelemetryEvent | null>(null);

  // Hash-based routing
  function getHashRoute(): string {
    const hash = window.location.hash;
    if (!hash || hash === '#' || hash === '#/') return '/status';
    return hash.slice(1); // remove leading #
  }

  const handleHashChange = () => {
    setRoute(getHashRoute());
  };

  createEffect(() => {
    window.addEventListener('hashchange', handleHashChange);
    onCleanup(() => window.removeEventListener('hashchange', handleHashChange));
  });

  // Ensure there is a hash on first load
  createEffect(() => {
    if (!window.location.hash || window.location.hash === '#' || window.location.hash === '#/') {
      window.location.hash = '#/status';
    }
  });

  // WebSocket connection
  createEffect(() => {
    const cleanup = connectWebSocket(
      (event) => {
        setLatestEvent(event);
      },
      () => {
        (window as any).__devrig_ws_connected = true;
      },
    );

    (window as any).__devrig_ws_connected = false;

    onCleanup(() => {
      (window as any).__devrig_ws_connected = false;
      cleanup();
    });
  });

  // Route matching helpers
  const routeSegment = () => {
    const r = route();
    if (r.startsWith('/traces/')) return 'trace-detail';
    if (r === '/status' || r === '/') return 'status';
    if (r === '/traces') return 'traces';
    if (r === '/logs') return 'logs';
    if (r === '/metrics') return 'metrics';
    if (r === '/config') return 'config';
    return 'status'; // fallback
  };

  const traceDetailId = () => {
    const r = route();
    if (r.startsWith('/traces/')) {
      return decodeURIComponent(r.slice('/traces/'.length));
    }
    return '';
  };

  return (
    <>
      <div data-testid="app-layout" class="flex h-screen bg-surface-0 text-text-primary font-sans max-[960px]:flex-col">
        {/* Sidebar */}
        <Sidebar currentRoute={route()} />

        {/* Main area */}
        <div class="flex flex-col flex-1 min-w-0 stencil-bg">
          {/* View content */}
          <main data-testid="main-content" class="flex-1 overflow-hidden bg-surface-0">
            <Switch fallback={<StatusView />}>
              <Match when={routeSegment() === 'traces'}>
                <TracesView onEvent={latestEvent()} />
              </Match>
              <Match when={routeSegment() === 'trace-detail'}>
                <TraceDetail traceId={traceDetailId()} />
              </Match>
              <Match when={routeSegment() === 'logs'}>
                <LogsView onEvent={latestEvent()} />
              </Match>
              <Match when={routeSegment() === 'metrics'}>
                <MetricsView onEvent={latestEvent()} />
              </Match>
              <Match when={routeSegment() === 'status'}>
                <StatusView />
              </Match>
              <Match when={routeSegment() === 'config'}>
                <ConfigView />
              </Match>
            </Switch>
          </main>

          {/* Status bar */}
          <StatusBar />
        </div>
      </div>
      <CommandPalette />
      <ToastProvider />
    </>
  );
};

export default App;
