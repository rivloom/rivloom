export type RuntimeStatus =
  | { state: "starting" }
  | {
      state: "connected";
      appVersion: string;
      appServerUserAgent: string;
      platform: string;
      codexHome: string;
    }
  | { state: "error"; message: string; retryable: boolean }
  | { state: "stopped" };
