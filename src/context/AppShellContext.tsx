import { createContext, useContext, type ReactNode } from "react";
import type { AppDepsState, AuthState, DependencyId } from "../types/backend";

export type LicenseIndicatorStatus =
  | "active"
  | "refresh_required"
  | "unlicensed";

export interface AppShellContextValue {
  authState: AuthState | null;
  isLicensed: boolean;
  licenseIndicatorStatus: LicenseIndicatorStatus;
  depsState: AppDepsState | null;
  subtitleCoreReady: boolean;
  missingSubtitleDependencies: DependencyId[];
}

const AppShellContext = createContext<AppShellContextValue | null>(null);

export function AppShellProvider({
  value,
  children,
}: {
  value: AppShellContextValue;
  children: ReactNode;
}) {
  return (
    <AppShellContext.Provider value={value}>
      {children}
    </AppShellContext.Provider>
  );
}

export function useAppShell() {
  const context = useContext(AppShellContext);
  if (!context) {
    throw new Error("useAppShell must be used within AppShellProvider");
  }
  return context;
}
