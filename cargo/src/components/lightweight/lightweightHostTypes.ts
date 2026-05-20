export interface SimpleDeliveryDefaults {
  runtimeUrl: string;
  publishPath: string;
  bindAddress: string;
}

export interface LightweightHostStatus {
  running: boolean;
  baseUrl: string;
  bindAddress: string;
  stagingPath: string;
  lastError?: string | null;
}
