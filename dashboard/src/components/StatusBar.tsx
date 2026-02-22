import { Component, createSignal, createEffect, onCleanup } from 'solid-js';
import { fetchStatus, type StatusResponse } from '../api';

const StatusBar: Component = () => {
  const [status, setStatus] = createSignal<StatusResponse | null>(null);
  const [wsConnected, setWsConnected] = createSignal(false);
  const [lastUpdated, setLastUpdated] = createSignal<string>('');

  const loadStatus = async () => {
    try {
      const data = await fetchStatus();
      setStatus(data);
      setLastUpdated(new Date().toLocaleTimeString());
    } catch {
      // silently handle - status bar is non-critical
    }
  };

  createEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 10000);
    onCleanup(() => clearInterval(interval));
  });

  // Monitor WebSocket connectivity by checking if the WS is established
  createEffect(() => {
    const checkWs = () => {
      // We track WS state through a global flag set by App
      setWsConnected(!!(window as any).__devrig_ws_connected);
    };
    checkWs();
    const interval = setInterval(checkWs, 2000);
    onCleanup(() => clearInterval(interval));
  });

  return (
    <footer class="h-8 bg-zinc-900 border-t border-zinc-700/50 flex items-center px-4 text-xs text-zinc-500 gap-6 shrink-0">
      <div class="flex items-center gap-1.5">
        <span
          class={`inline-block w-2 h-2 rounded-full ${
            wsConnected() ? 'bg-green-500' : 'bg-zinc-600'
          }`}
        />
        <span>{wsConnected() ? 'Live' : 'Disconnected'}</span>
      </div>

      {status() && (
        <>
          <div class="flex items-center gap-1">
            <span class="text-zinc-600">Traces:</span>
            <span class="text-zinc-400">{status()!.trace_count.toLocaleString()}</span>
          </div>
          <div class="flex items-center gap-1">
            <span class="text-zinc-600">Spans:</span>
            <span class="text-zinc-400">{status()!.span_count.toLocaleString()}</span>
          </div>
          <div class="flex items-center gap-1">
            <span class="text-zinc-600">Logs:</span>
            <span class="text-zinc-400">{status()!.log_count.toLocaleString()}</span>
          </div>
          <div class="flex items-center gap-1">
            <span class="text-zinc-600">Metrics:</span>
            <span class="text-zinc-400">{status()!.metric_count.toLocaleString()}</span>
          </div>
          <div class="flex items-center gap-1">
            <span class="text-zinc-600">Services:</span>
            <span class="text-zinc-400">{status()!.services.length}</span>
          </div>
        </>
      )}

      <div class="ml-auto text-zinc-600">
        {lastUpdated() && `Updated ${lastUpdated()}`}
      </div>
    </footer>
  );
};

export default StatusBar;
