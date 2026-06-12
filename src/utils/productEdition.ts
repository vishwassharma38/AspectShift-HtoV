import type { AuthState, AuthStatus } from "../types/backend";

export type ProductEdition = "Pro" | "Community";

const LICENSED_STATUSES = new Set<AuthStatus>([
  "valid",
  "offline_valid",
  "grace_period",
  "refresh_required",
]);

export function resolveProductEdition(
  status: AuthStatus | null | undefined,
): ProductEdition {
  return status && LICENSED_STATUSES.has(status) ? "Pro" : "Community";
}

export function resolveProductEditionFromAuthState(
  authState: Pick<AuthState, "status"> | null | undefined,
): ProductEdition {
  return resolveProductEdition(authState?.status);
}
