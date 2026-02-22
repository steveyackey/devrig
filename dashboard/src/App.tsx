import { Component, createSignal, createEffect, onCleanup, Show, Switch, Match } from 'solid-js';
import { connectWebSocket, type TelemetryEvent } from './api';
import Sidebar from './components/Sidebar';
import StatusBar from './components/StatusBar';
import TracesView from './views/TracesView';
import TraceDetail from './views/TraceDetail';
import LogsView from './views/LogsView';
import MetricsView from './views/MetricsView';
import StatusView from './views/StatusView';

const App: Component = () => {
  const [route, setRoute] = createSignal(getHashRoute());
  const [latestEvent, setLatestEvent] = createSignal<TelemetryEvent | null>(null);

  // Hash-based routing
  function getHashRoute(): string {
    const hash = window.location.hash;
    if (!hash || hash === '#' || hash === '#/') return '/traces';
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
      window.location.hash = '#/traces';
    }
  });

  // WebSocket connection
  createEffect(() => {
    const cleanup = connectWebSocket((event) => {
      (window as any).__devrig_ws_connected = true;
      setLatestEvent(event);
    });

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
    if (r === '/traces' || r === '/') return 'traces';
    if (r === '/logs') return 'logs';
    if (r === '/metrics') return 'metrics';
    if (r === '/status') return 'status';
    return 'traces'; // fallback
  };

  const traceDetailId = () => {
    const r = route();
    if (r.startsWith('/traces/')) {
      return decodeURIComponent(r.slice('/traces/'.length));
    }
    return '';
  };

  return (
    <div class="flex h-screen bg-zinc-900 text-zinc-200 font-sans">
      {/* Sidebar */}
      <Sidebar currentRoute={route()} />

      {/* Main area */}
      <div class="flex flex-col flex-1 min-w-0">
        {/* View content */}
        <main class="flex-1 overflow-hidden bg-zinc-900">
          <Switch fallback={<TracesView onEvent={latestEvent()} />}>
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
          </Switch>
        </main>

        {/* Status bar */}
        <StatusBar />
      </div>
    </div>
  );
};

export default App;
