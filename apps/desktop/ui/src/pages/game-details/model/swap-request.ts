/** Complete request for one component replacement. */
export type SwapRequest = {
  componentId: string;
  artifactId: string;
  isDownloaded: boolean;
  confirmationToken?: string | null;
};
