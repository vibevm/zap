/** Provider proxy policy contracts. @scope spec://org.vibevm.zap/lens/PROP-010#agent-network */
import { z } from "zod";

export const ProxyModeSchema = z.enum(["inherit", "direct", "explicit"]);
export type ProxyMode = z.infer<typeof ProxyModeSchema>;
const ProxyUrlSchema = z
  .url()
  .max(2_048)
  .refine((value) => {
    const protocol = new URL(value).protocol;
    return protocol === "http:" || protocol === "https:";
  }, "proxy URL must use HTTP or HTTPS")
  .refine(
    (value) => !new URL(value).username && !new URL(value).password,
    "proxy credentials are not allowed",
  );
export const ProxyPolicySchema = z
  .object({
    mode: ProxyModeSchema,
    httpProxy: ProxyUrlSchema.optional(),
    httpsProxy: ProxyUrlSchema.optional(),
    allProxy: ProxyUrlSchema.optional(),
    noProxy: z.string().max(8_000).optional(),
  })
  .strict()
  .superRefine((policy, context) => {
    if (
      policy.mode === "explicit" &&
      policy.httpProxy === undefined &&
      policy.httpsProxy === undefined &&
      policy.allProxy === undefined
    )
      context.addIssue({ code: "custom", message: "explicit proxy mode requires a proxy URL" });
    if (
      policy.mode !== "explicit" &&
      (policy.httpProxy !== undefined ||
        policy.httpsProxy !== undefined ||
        policy.allProxy !== undefined)
    )
      context.addIssue({ code: "custom", message: "proxy URLs are only valid in explicit mode" });
  });
export type ProxyPolicy = z.infer<typeof ProxyPolicySchema>;

export interface ProxyResolutionInput {
  readonly ambient: Readonly<Record<string, string | undefined>>;
  readonly global?: ProxyPolicy;
  readonly profile?: ProxyPolicy;
}

export interface ProxyResolution {
  readonly mode: ProxyMode;
  readonly environment: Readonly<Record<string, string | undefined>>;
  readonly source: "ambient" | "direct" | "explicit";
}
