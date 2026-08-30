export type CodexRuntimeAuthStatus =
  | { state: "checking" }
  | { state: "signedOut" }
  | { state: "browserPending" }
  | {
      state: "signedIn";
      email: string | null;
      planType: string;
    }
  | { state: "error"; message: string; retryable: boolean };
