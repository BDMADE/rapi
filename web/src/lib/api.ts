import { useAuthStore } from "@/features/auth/auth-store";

export interface ApiResponse<T> {
  data: T;
  meta: {
    pagination?: {
      page: number;
      pageSize: number;
      pageCount: number;
      total: number;
    };
  };
}

export interface ApiErrorEnvelope {
  data: null;
  error: {
    status: number;
    name: string;
    message: string;
    details: Record<string, unknown>;
  };
}

export type RuntimeBackendKind = "local" | "redis";

export interface PlatformStats {
  contentTypes: number;
  documents: number;
  publishedDocuments: number;
  mediaFiles: number;
  locales: number;
  adminUsers: number;
  webhooks: number;
  pendingJobs: number;
}

export interface SystemOverviewDto {
  version: string;
  sqliteJournalMode: string;
  cacheBackend: RuntimeBackendKind;
  queueBackend: RuntimeBackendKind;
  uptimeSeconds: number;
  hasAdmin: boolean;
  defaultLocale: string;
  stats: PlatformStats;
}

export interface ReadinessDto {
  status: string;
  database: string;
  cacheBackend: RuntimeBackendKind;
  queueBackend: RuntimeBackendKind;
}

export interface AdminInitDto {
  hasAdmin: boolean;
  version: string;
  defaultLocale: string;
  runtimeBackend: RuntimeBackendKind;
}

export interface AdminRoleSummary {
  id: string;
  code: string;
  name: string;
}

export interface AdminUserDto {
  id: string;
  email: string;
  username: string | null;
  firstname: string;
  lastname: string;
  isActive: boolean;
  isSuperAdmin: boolean;
  roles: AdminRoleSummary[];
  lastLoginAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AdminRoleDto {
  id: string;
  code: string;
  name: string;
  description: string;
  userCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface PermissionRule {
  action: string;
  targetUid?: string | null;
  fields?: string[];
}

export interface PermissionCatalogItem {
  action: string;
  label: string;
  category: string;
  description: string;
}

export type TokenType = "full-access" | "read-only" | "custom";

export interface ApiTokenDto {
  id: string;
  name: string;
  description: string;
  tokenType: TokenType;
  lifespanMs: number | null;
  expiresAt: string | null;
  lastUsedAt: string | null;
  permissions: PermissionRule[];
  createdAt: string;
  updatedAt: string;
}

export interface CreatedTokenDto extends ApiTokenDto {
  accessKey: string;
}

export interface AuthTokenResponse {
  token: string;
  refreshToken: string;
  user: AdminUserDto;
  permissions: PermissionRule[];
}

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? "";

async function tryRefreshSession(): Promise<string | null> {
  const { refreshToken, setSession, clearSession } = useAuthStore.getState();
  if (!refreshToken) return null;

  try {
    const res = await fetch(`${API_BASE}/admin/refresh-token`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refreshToken }),
    });
    if (!res.ok) {
      clearSession();
      return null;
    }
    const json = (await res.json()) as ApiResponse<AuthTokenResponse>;
    setSession(json.data);
    return json.data.token;
  } catch {
    clearSession();
    return null;
  }
}

async function requestJson<T>(
  path: string,
  init?: RequestInit,
  allowRetry = true,
): Promise<T> {
  const token = useAuthStore.getState().token;
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...((init?.headers as Record<string, string> | undefined) ?? {}),
  };
  if (token && !headers.Authorization) {
    headers.Authorization = `Bearer ${token}`;
  }

  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers,
  });

  if (res.status === 401 && allowRetry && path !== "/admin/login" && path !== "/admin/refresh-token") {
    const newToken = await tryRefreshSession();
    if (newToken) {
      return requestJson<T>(path, init, false);
    }
  }

  if (!res.ok) {
    let errMsg = `HTTP ${res.status}`;
    try {
      const errBody = (await res.json()) as ApiErrorEnvelope;
      if (errBody?.error?.message) {
        errMsg = errBody.error.message;
      }
    } catch {
      // fallback to status code
    }
    throw new Error(errMsg);
  }

  return (await res.json()) as T;
}

export const api = {
  getReadiness: () => requestJson<ReadinessDto>("/ready"),
  getAdminInit: async () => (await requestJson<ApiResponse<AdminInitDto>>("/admin/init")).data,
  getSystemOverview: async () =>
    (await requestJson<ApiResponse<SystemOverviewDto>>("/admin/system/overview")).data,

  // Auth
  registerAdmin: async (payload: {
    email: string;
    password: string;
    firstname: string;
    lastname?: string;
  }) =>
    (
      await requestJson<ApiResponse<AuthTokenResponse>>("/admin/register-admin", {
        method: "POST",
        body: JSON.stringify(payload),
      })
    ).data,

  login: async (payload: { email: string; password: string }) =>
    (
      await requestJson<ApiResponse<AuthTokenResponse>>("/admin/login", {
        method: "POST",
        body: JSON.stringify(payload),
      })
    ).data,

  logout: async (refreshToken: string) =>
    requestJson<ApiResponse<{ ok: boolean }>>("/admin/logout", {
      method: "POST",
      body: JSON.stringify({ refreshToken }),
    }),

  // Users
  listAdminUsers: async () =>
    (await requestJson<ApiResponse<AdminUserDto[]>>("/admin/users")).data,

  createAdminUser: async (payload: {
    email: string;
    password: string;
    firstname: string;
    lastname?: string;
    roleIds: string[];
  }) =>
    (
      await requestJson<ApiResponse<AdminUserDto>>("/admin/users", {
        method: "POST",
        body: JSON.stringify(payload),
      })
    ).data,

  deleteAdminUser: async (id: string) =>
    requestJson<ApiResponse<{ deleted: string }>>(`/admin/users/${id}`, {
      method: "DELETE",
    }),

  // Roles & Permissions
  listRoles: async () => (await requestJson<ApiResponse<AdminRoleDto[]>>("/admin/roles")).data,

  createRole: async (payload: { name: string; description: string }) =>
    (
      await requestJson<ApiResponse<AdminRoleDto>>("/admin/roles", {
        method: "POST",
        body: JSON.stringify(payload),
      })
    ).data,

  deleteRole: async (id: string) =>
    requestJson<ApiResponse<{ deleted: string }>>(`/admin/roles/${id}`, {
      method: "DELETE",
    }),

  getPermissionCatalog: async () =>
    (await requestJson<ApiResponse<PermissionCatalogItem[]>>("/admin/permissions/catalog")).data,

  getRolePermissions: async (roleId: string) =>
    (await requestJson<ApiResponse<PermissionRule[]>>(`/admin/roles/${roleId}/permissions`)).data,

  updateRolePermissions: async (roleId: string, permissions: PermissionRule[]) =>
    (
      await requestJson<ApiResponse<PermissionRule[]>>(`/admin/roles/${roleId}/permissions`, {
        method: "PUT",
        body: JSON.stringify({ permissions }),
      })
    ).data,

  // API & Admin Tokens
  listTokens: async (kind: "api" | "admin") =>
    (
      await requestJson<ApiResponse<ApiTokenDto[]>>(
        kind === "admin" ? "/admin/admin-tokens" : "/admin/api-tokens",
      )
    ).data,

  createToken: async (
    kind: "api" | "admin",
    payload: {
      name: string;
      description: string;
      type: TokenType;
      lifespanMs: number | null;
      permissions?: PermissionRule[];
    },
  ) =>
    (
      await requestJson<ApiResponse<CreatedTokenDto>>(
        kind === "admin" ? "/admin/admin-tokens" : "/admin/api-tokens",
        {
          method: "POST",
          body: JSON.stringify(payload),
        },
      )
    ).data,

  deleteToken: async (kind: "api" | "admin", id: string) =>
    requestJson<ApiResponse<{ deleted: string }>>(
      kind === "admin" ? `/admin/admin-tokens/${id}` : `/admin/api-tokens/${id}`,
      { method: "DELETE" },
    ),
};
