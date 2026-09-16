/** Authenticated product setup gateway operation. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import {
  ProductSetupRequestSchema,
  ProductSetupResponseSchema,
  type ProductSetupPort,
  type ProductSetupResult,
  type ProductSetupResponse,
} from "../workspace-model/index.ts";

export async function productGatewayOperation(
  path: string,
  value: unknown,
  source: ProductSetupPort | undefined,
): Promise<ProductSetupResult<ProductSetupResponse>> {
  if (source === undefined) return failure("unavailable", "Product setup is not configured");
  if (path !== "/v1/product/request")
    return failure("not_found", "Product setup route is unavailable");
  const request = ProductSetupRequestSchema.safeParse(value);
  if (!request.success) return failure("invalid_input", "Product setup request is invalid");
  const response = await source.request(request.data);
  if (!response.ok) return response;
  const parsed = ProductSetupResponseSchema.safeParse(response.value);
  return parsed.success
    ? { ok: true, value: parsed.data }
    : failure("unavailable", "Product setup returned invalid data");
}

function failure(
  code: "invalid_input" | "not_found" | "conflict" | "unavailable",
  message: string,
): ProductSetupResult<never> {
  return { ok: false, error: { code, message } };
}
