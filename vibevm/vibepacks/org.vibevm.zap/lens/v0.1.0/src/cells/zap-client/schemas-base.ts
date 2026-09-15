/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { z } from "zod";

const ID_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._:-]*$/;
const DIGEST_PATTERN = /^[0-9a-f]{64}$/;
export const ZapIdSchema = z.string().min(1).max(1024).regex(ID_PATTERN).brand<"ZapId">();
export const ZapDigestSchema = z.string().regex(DIGEST_PATTERN).brand<"ZapDigest">();
