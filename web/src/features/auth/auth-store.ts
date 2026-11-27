import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { AdminUserDto, PermissionRule } from "@/lib/api";

interface AuthState {
  token: string | null;
  refreshToken: string | null;
  user: AdminUserDto | null;
  permissions: PermissionRule[];
  setSession: (payload: {
    token: string;
    refreshToken: string;
    user: AdminUserDto;
    permissions: PermissionRule[];
  }) => void;
  clearSession: () => void;
  can: (action: string) => boolean;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      refreshToken: null,
      user: null,
      permissions: [],
      setSession: ({ token, refreshToken, user, permissions }) =>
        set({ token, refreshToken, user, permissions }),
      clearSession: () =>
        set({ token: null, refreshToken: null, user: null, permissions: [] }),
      can: (action: string) => {
        const { user, permissions } = get();
        if (!user) return false;
        if (user.isSuperAdmin) return true;
        return permissions.some((p) => p.action === "*" || p.action === action);
      },
    }),
    {
      name: "rapi_admin_session",
    },
  ),
);
