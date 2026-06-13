import type { TelemetryEvent } from "@/api";

export interface WebSocketClientOptions {
  onEvent: (event: TelemetryEvent) => void;
  onStatusChange?: (connected: boolean) => void;
  reconnectDelay?: number;
  maxReconnectDelay?: number;
  url?: string;
}

export interface WebSocketClient {
  close: () => void;
  connected: () => boolean;
}

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
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
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
    const delay = Math.min(reconnectDelay * Math.pow(2, attempt), maxReconnectDelay);
    attempt++;
    reconnectTimer = setTimeout(connect, delay);
  }

  function connect() {
    if (closed) return;
    try {
      ws = new WebSocket(getUrl());
    } catch {
      console.warn("[devrig:ws] Failed to construct WebSocket, will retry");
      scheduleReconnect();
      return;
    }

    ws.onopen = () => {
      console.log("[devrig:ws] Connected");
      attempt = 0;
      setConnected(true);
      (window as unknown as Record<string, unknown>)["__devrig_ws_connected"] = true;
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data as string) as TelemetryEvent;
        onEvent(data);
      } catch (err) {
        console.warn("[devrig:ws] Failed to parse message:", err);
      }
    };

    ws.onclose = () => {
      setConnected(false);
      (window as unknown as Record<string, unknown>)["__devrig_ws_connected"] = false;
      if (!closed) {
        console.log("[devrig:ws] Disconnected, scheduling reconnect...");
        scheduleReconnect();
      }
    };

    ws.onerror = (err) => {
      console.warn("[devrig:ws] Error:", err);
      ws?.close();
    };
  }

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
      (window as unknown as Record<string, unknown>)["__devrig_ws_connected"] = false;
    },
    connected: () => isConnected,
  };
}
