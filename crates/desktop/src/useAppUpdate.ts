import { useCallback, useEffect, useRef, useState } from "react";
import { checkAppUpdate, installAppUpdate } from "./desktopClient";
import type { AppUpdateMetadata } from "./types";

export interface AppUpdateController {
  available: AppUpdateMetadata | null;
  check: () => Promise<void>;
  checking: boolean;
  close: () => void;
  error: string | null;
  install: () => Promise<void>;
  installing: boolean;
  isOpen: boolean;
  open: () => void;
}

export function useAppUpdate(): AppUpdateController {
  const [available, setAvailable] = useState<AppUpdateMetadata | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const checkingRef = useRef(false);
  const mountedRef = useRef(false);

  const check = useCallback(async () => {
    if (checkingRef.current) return;
    checkingRef.current = true;
    setChecking(true);
    try {
      const update = await checkAppUpdate();
      if (!mountedRef.current) return;
      setAvailable(update);
      if (update) setIsOpen(true);
    } catch (reason) {
      console.warn("[app-update] Could not check for updates", reason);
    } finally {
      checkingRef.current = false;
      if (mountedRef.current) setChecking(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void check();
    return () => { mountedRef.current = false; };
  }, [check]);

  const install = useCallback(async () => {
    if (!available || installing) return;
    setInstalling(true);
    setError(null);
    try {
      await installAppUpdate(available.version);
    } catch (reason) {
      setError(errorMessage(reason));
      setInstalling(false);
    }
  }, [available, installing]);

  return {
    available,
    check,
    checking,
    close: () => setIsOpen(false),
    error,
    install,
    installing,
    isOpen,
    open: () => {
      if (available) setIsOpen(true);
      else void check();
    },
  };
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) return String(error.message);
  return String(error);
}
