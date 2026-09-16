/** @scope spec://org.vibevm.zap/lens/PROP-002#read-model */
const REQUIREMENT = "spec://org.vibevm.zap/lens/PROP-002#read-model";

export function reqMessage(uri: string, why: string, fixSurface: string): string {
  return `violates REQ ${uri}: ${why}; fix surface: ${fixSurface}`;
}

export function diagnostic(why: string): Error {
  return new Error(reqMessage(REQUIREMENT, why, "refresh from the configured ZAP service"));
}
