import { invoke } from "@tauri-apps/api/core";

import type { RivloomIdentity } from "../types/identity";

export function getIdentity(): Promise<RivloomIdentity> {
  return invoke<RivloomIdentity>("get_identity");
}
