/**
 * WebSocket client with automatic reconnection for the DevRig telemetry stream.
 *
 * Usage:
 *   import { createWebSocket } from '../lib/ws';
 *
 *   const ws = createWebSocket({
 *     onEvent: (event) => console.log(event),
 *     onStatusChange: (connected) => console.log('ws connected:', connected),
 *   });
 *
 *   // Later, to tear down:
 *   ws.close();
 */

import type { TelemetryEvent } from '../api';

export interface WebSocketClientOptions {
  /** Called when a parsed TelemetryEvent arrives. */
  onEvent: (event: TelemetryEvent) => void;
  /** Called when the connection status changes. */
  onStatusChange?: (connected: boolean) => void;
  /** Base delay in ms before the first reconnect attempt. Default: 1000. */
  reconnectDelay?: number;
  /** Maximum reconnect delay in ms (exponential back-off cap). Default: 30000. */
  maxReconnectDelay?: number;
  /** Override the WebSocket URL (useful for tests). */
  url?: string;
}

export interface WebSocketClient {
  /** Manually close the connection and stop reconnecting. */
  close: () => void;
  /** Whether the socket is currently connected. */
  connected: () => boolean;
}

/**
 * Creates a managed WebSocket connection to the DevRig telemetry stream.
 * Automatically reconnects with exponential back-off when the connection drops.
 */
export function createWebSocket(options: WebSocketClientOptions): WebSocketClient {
  const {
    onEvent,
    onStatusChange,
    reconnectDelay = 1000,
    maxReconnectDelay = 30000,
    url,
  } = options;

  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let closed = false;
  let isConnected = false;
  let attempt = 0;

  function getUrl(): string {
    if (url) return url;
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${protocol}//${window.location.host}/ws`;
  }

  function setConnected(value: boolean) {
    if (isConnected !== value) {
      isConnected = value;
      onStatusChange?.(value);
    }
  }

  function scheduleReconnect() {
    if (closed) return;
    // Exponential back-off: delay * 2^attempt, capped at maxReconnectDelay
    const delay = Math.min(reconnectDelay * Math.pow(2, attempt), maxReconnectDelay);
    attempt++;
    reconnectTimer = setTimeout(connect, delay);
  }

  function connect() {
    if (closed) return;

    try {
      ws = new WebSocket(getUrl());
    } catch {
      console.warn('[devrig:ws] Failed to construct WebSocket, will retry');
      scheduleReconnect();
      return;
    }

    ws.onopen = () => {
      console.log('[devrig:ws] Connected');
      attempt = 0; // reset back-off on successful connection
      setConnected(true);
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as TelemetryEvent;
        onEvent(data);
      } catch (err) {
        console.warn('[devrig:ws] Failed to parse message:', err);
      }
    };

    ws.onclose = () => {
      setConnected(false);
      if (!closed) {
        console.log('[devrig:ws] Disconnected, scheduling reconnect...');
        scheduleReconnect();
      }
    };

    ws.onerror = (err) => {
      console.warn('[devrig:ws] Error:', err);
      // onclose will fire after onerror, which handles reconnection
      ws?.close();
    };
  }

  // Start the initial connection
  connect();

  return {
    close: () => {
      closed = true;
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      ws?.close();
      setConnected(false);
    },
    connected: () => isConnected,
  };
}

export type { TelemetryEvent };
