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

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? "";

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    ...init,
  });

  if (!res.ok) {
    let errMsg = `HTTP ${res.status}`;
    try {
      const errBody = (await res.json()) as ApiErrorEnvelope;
      if (errBody?.error?.message) {
        errMsg = errBody.error.message;
      }
    } catch {
      // fallback to status
    }
    throw new Error(errMsg);
  }

  return (await res.json()) as T;
}

export const api = {
  getReadiness: () => requestJson<ReadinessDto>("/ready"),
  getAdminInit: async () => {
    const res = await requestJson<ApiResponse<AdminInitDto>>("/admin/init");
    return res.data;
  },
  getSystemOverview: async () => {
    const res = await requestJson<ApiResponse<SystemOverviewDto>>("/admin/system/overview");
    return res.data;
  },
};
