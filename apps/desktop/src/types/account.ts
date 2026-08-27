export type AccountStatus =
  | { state: "checking" }
  | { state: "signedOut" }
  | { state: "browserPending" }
  | {
      state: "devicePending";
      verificationUrl: string;
      userCode: string;
    }
  | {
      state: "signedIn";
      email: string | null;
      planType: string;
    }
  | { state: "error"; message: string; retryable: boolean };
