import { useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";

/**
 * Generic hook to listen for Tauri events.
 * Automatically cleans up the listener on unmount.
 */
export function useTauriEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const setup = async () => {
      unlisten = await listen<T>(eventName, (event) => {
        handlerRef.current(event.payload);
      });
    };

    setup();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [eventName]);
}

/**
 * Hook to track Tauri event state.
 * Returns the latest event payload.
 */
export function useTauriEventState<T>(eventName: string, initialValue: T): T {
  const valueRef = useRef<T>(initialValue);

  const handler = useCallback((payload: T) => {
    valueRef.current = payload;
  }, []);

  useTauriEvent(eventName, handler);

  return valueRef.current;
}
