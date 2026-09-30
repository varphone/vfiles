export interface ErrorBoundaryActions {
  log: (source: string, error: unknown) => void;
  incrementErrorCount: () => void;
  notify: (message: string) => void;
  report: (source: string, error: unknown) => void;
  now?: () => number;
}

const TOAST_THROTTLE_MS = 3000;
const ERROR_NOTIFICATION = "界面出现异常，操作未受影响，可继续使用";

export function createErrorBoundary(actions: ErrorBoundaryActions) {
  let lastToastAt: number | undefined;
  const now = actions.now ?? Date.now;

  return (source: string, error: unknown) => {
    actions.log(source, error);
    actions.incrementErrorCount();

    const timestamp = now();
    if (
      lastToastAt === undefined ||
      timestamp - lastToastAt > TOAST_THROTTLE_MS
    ) {
      lastToastAt = timestamp;
      try {
        actions.notify(ERROR_NOTIFICATION);
      } catch {
        // Error reporting must remain safe when the notification UI has failed.
      }
    }

    try {
      actions.report(source, error);
    } catch {
      // Reporting failures must not trigger another uncaught UI error.
    }
  };
}
