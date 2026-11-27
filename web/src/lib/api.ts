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

export type ContentKind = "collectionType" | "singleType" | "component";

export type FieldType =
  | "string"
  | "text"
  | "richtext"
  | "blocks"
  | "integer"
  | "biginteger"
  | "float"
  | "decimal"
  | "boolean"
  | "date"
  | "datetime"
  | "time"
  | "email"
  | "password"
  | "enumeration"
  | "json"
  | "uid"
  | "media"
  | "relation"
  | "component"
  | "dynamiczone"
  | "custom";

export type RelationKind =
  | "oneToOne"
  | "oneToMany"
  | "manyToOne"
  | "manyToMany"
  | "morphOne"
  | "morphMany";

export interface FieldValidation {
  minLength?: number;
  maxLength?: number;
  min?: number;
  max?: number;
  regex?: string;
  enumValues?: string[];
  targetField?: string;
  multiple?: boolean;
  allowedTypes?: string[];
  conditions?: {
    visible?: {
      field: string;
      eq?: unknown;
      ne?: unknown;
    };
  };
}

export interface RelationConfig {
  relationType: RelationKind;
  targetUid?: string;
  inversedBy?: string;
  mappedBy?: string;
}

export interface ComponentConfig {
  componentUid?: string;
  repeatable?: boolean;
  components?: string[];
  min?: number;
  max?: number;
}

export interface FieldDefinition {
  name: string;
  fieldType: FieldType;
  required?: boolean;
  unique?: boolean;
  localized?: boolean;
  private?: boolean;
  defaultValue?: unknown;
  validation?: FieldValidation;
  relation?: RelationConfig;
  component?: ComponentConfig;
  customFieldUid?: string;
}

export interface ContentTypeSchema {
  uid: string;
  kind: ContentKind;
  category?: string;
  singularName: string;
  pluralName: string;
  displayName: string;
  description?: string;
  icon?: string;
  draftAndPublish: boolean;
  i18nEnabled: boolean;
  pluginOptions?: Record<string, unknown>;
  options?: Record<string, unknown>;
  fields: FieldDefinition[];
  createdAt?: string;
  updatedAt?: string;
}

export interface SchemaCatalogResponse {
  contentTypes: ContentTypeSchema[];
  components: ContentTypeSchema[];
}

export interface PhysicalColumnInfo {
  cid: number;
  name: string;
  dataType: string;
  notNull: boolean;
  primaryKey: boolean;
}

export interface SchemaMigrationRecord {
  id: number;
  contentTypeUid: string;
  operation: string;
  ddlSql: string;
  appliedAt: string;
}

export interface SchemaInspectionDto {
  uid: string;
  tableName: string | null;
  ftsTableName: string | null;
  columns: PhysicalColumnInfo[];
  migrations: SchemaMigrationRecord[];
}

export interface ContentVersionDto {
  id: string;
  documentId: string;
  contentTypeUid: string;
  version: number;
  locale: string;
  status: string;
  data: Record<string, unknown>;
  createdBy: string | null;
  createdAt: string;
}

export type ContentDocument = Record<string, unknown> & {
  id: number;
  documentId: string;
  locale: string;
  publishedAt: string | null;
  publicationState?: "draft" | "published" | "modified";
  createdAt: string;
  updatedAt: string;
};

export interface MediaFormatVariant {
  name: string;
  hash: string;
  ext: string;
  mime: string;
  width: number;
  height: number;
  size: number;
  url: string;
}

export interface MediaFileDto {
  id: number;
  documentId: string;
  name: string;
  alternativeText: string | null;
  caption: string | null;
  width: number | null;
  height: number | null;
  formats: Record<string, MediaFormatVariant>;
  hash: string;
  ext: string;
  mime: string;
  size: number;
  url: string;
  previewUrl: string | null;
  provider: string;
  folderId: number | null;
  folderPath: string;
  usageCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface MediaFolderDto {
  id: number;
  name: string;
  pathId: number;
  path: string;
  parentId: number | null;
  fileCount: number;
  childCount: number;
  createdAt: string;
  updatedAt: string;
}

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? "";

// Deduplicates concurrent 401 refresh attempts so single-use refresh token rotation
// does not race when multiple parallel queries fire on an expired JWT.
let inflightRefresh: Promise<string | null> | null = null;

async function tryRefreshSession(): Promise<string | null> {
  if (inflightRefresh) {
    return inflightRefresh;
  }

  inflightRefresh = (async () => {
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
    } finally {
      inflightRefresh = null;
    }
  })();

  return inflightRefresh;
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

  updateAdminUser: async (
    id: string,
    payload: {
      firstname?: string;
      lastname?: string;
      isActive?: boolean;
      password?: string;
      roleIds?: string[];
    },
  ) =>
    (
      await requestJson<ApiResponse<AdminUserDto>>(`/admin/users/${id}`, {
        method: "PUT",
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

  // Content-Type Builder (Schemas & Components)
  listSchemas: async () =>
    (await requestJson<ApiResponse<SchemaCatalogResponse>>("/admin/content-types")).data,

  getSchema: async (uid: string) =>
    (
      await requestJson<ApiResponse<ContentTypeSchema>>(
        `/admin/content-types/${encodeURIComponent(uid)}`,
      )
    ).data,

  createSchema: async (schema: ContentTypeSchema) =>
    (
      await requestJson<ApiResponse<ContentTypeSchema>>("/admin/content-types", {
        method: "POST",
        body: JSON.stringify(schema),
      })
    ).data,

  updateSchema: async (uid: string, schema: ContentTypeSchema) =>
    (
      await requestJson<ApiResponse<ContentTypeSchema>>(
        `/admin/content-types/${encodeURIComponent(uid)}`,
        {
          method: "PUT",
          body: JSON.stringify(schema),
        },
      )
    ).data,

  deleteSchema: async (uid: string) =>
    requestJson<ApiResponse<{ deleted: string }>>(
      `/admin/content-types/${encodeURIComponent(uid)}`,
      { method: "DELETE" },
    ),

  inspectSchema: async (uid: string) =>
    (
      await requestJson<ApiResponse<SchemaInspectionDto>>(
        `/admin/content-types/${encodeURIComponent(uid)}/inspection`,
      )
    ).data,

  // Public API Role Permissions
  getPublicPermissions: async () =>
    (await requestJson<ApiResponse<PermissionRule[]>>("/admin/public-permissions")).data,

  updatePublicPermissions: async (permissions: PermissionRule[]) =>
    (
      await requestJson<ApiResponse<PermissionRule[]>>("/admin/public-permissions", {
        method: "PUT",
        body: JSON.stringify({ permissions }),
      })
    ).data,

  // Admin Content Manager
  listEntries: async (
    uid: string,
    params: {
      page?: number;
      pageSize?: number;
      locale?: string;
      status?: "draft" | "published";
      q?: string;
      sort?: string;
    } = {},
  ) => {
    const qs = new URLSearchParams();
    if (params.page) qs.set("pagination[page]", String(params.page));
    if (params.pageSize) qs.set("pagination[pageSize]", String(params.pageSize));
    if (params.locale) qs.set("locale", params.locale);
    if (params.status) qs.set("status", params.status);
    if (params.q) qs.set("_q", params.q);
    if (params.sort) qs.set("sort", params.sort);
    const suffix = qs.toString() ? `?${qs.toString()}` : "";
    return requestJson<ApiResponse<ContentDocument[]>>(
      `/admin/content-manager/${encodeURIComponent(uid)}${suffix}`,
    );
  },

  getEntry: async (uid: string, documentId: string, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}?locale=${encodeURIComponent(locale)}`,
      )
    ).data,

  getSingleEntry: async (uid: string, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentDocument | null>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/single?locale=${encodeURIComponent(locale)}`,
      )
    ).data,

  createEntry: async (uid: string, data: Record<string, unknown>, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}?locale=${encodeURIComponent(locale)}`,
        {
          method: "POST",
          body: JSON.stringify({ data }),
        },
      )
    ).data,

  updateEntry: async (
    uid: string,
    documentId: string,
    data: Record<string, unknown>,
    locale = "en",
  ) =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}?locale=${encodeURIComponent(locale)}`,
        {
          method: "PUT",
          body: JSON.stringify({ data }),
        },
      )
    ).data,

  upsertSingleEntry: async (uid: string, data: Record<string, unknown>, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/single?locale=${encodeURIComponent(locale)}`,
        {
          method: "PUT",
          body: JSON.stringify({ data }),
        },
      )
    ).data,

  deleteEntry: async (uid: string, documentId: string) =>
    requestJson<ApiResponse<{ deleted: string }>>(
      `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}`,
      { method: "DELETE" },
    ),

  publishEntry: async (uid: string, documentId: string, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}/publish?locale=${encodeURIComponent(locale)}`,
        { method: "POST" },
      )
    ).data,

  unpublishEntry: async (uid: string, documentId: string, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}/unpublish?locale=${encodeURIComponent(locale)}`,
        { method: "POST" },
      )
    ).data,

  bulkPublishEntries: async (uid: string, documentIds: string[], locale = "en") =>
    requestJson<ApiResponse<{ publishedCount: number }>>(
      `/admin/content-manager/${encodeURIComponent(uid)}/bulk-publish`,
      {
        method: "POST",
        body: JSON.stringify({ documentIds, locale }),
      },
    ),

  bulkDeleteEntries: async (uid: string, documentIds: string[]) =>
    requestJson<ApiResponse<{ deletedCount: number }>>(
      `/admin/content-manager/${encodeURIComponent(uid)}/bulk-delete`,
      {
        method: "POST",
        body: JSON.stringify({ documentIds }),
      },
    ),

  listEntryVersions: async (uid: string, documentId: string, locale = "en") =>
    (
      await requestJson<ApiResponse<ContentVersionDto[]>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}/versions?locale=${encodeURIComponent(locale)}`,
      )
    ).data,

  restoreEntryVersion: async (uid: string, documentId: string, versionId: string) =>
    (
      await requestJson<ApiResponse<ContentDocument>>(
        `/admin/content-manager/${encodeURIComponent(uid)}/${encodeURIComponent(documentId)}/versions/${encodeURIComponent(versionId)}/restore`,
        { method: "POST" },
      )
    ).data,

  // Media Library
  listMediaFiles: async (
    params: {
      folderId?: number | "root";
      mimePrefix?: string;
      q?: string;
      page?: number;
      pageSize?: number;
    } = {},
  ) => {
    const qs = new URLSearchParams();
    if (params.folderId !== undefined) qs.set("folderId", String(params.folderId));
    if (params.mimePrefix) qs.set("mimePrefix", params.mimePrefix);
    if (params.q) qs.set("_q", params.q);
    if (params.page) qs.set("page", String(params.page));
    if (params.pageSize) qs.set("pageSize", String(params.pageSize));
    const suffix = qs.toString() ? `?${qs.toString()}` : "";
    return requestJson<ApiResponse<MediaFileDto[]>>(`/admin/media/files${suffix}`);
  },

  uploadMediaFiles: async (files: File[], folderId?: number | null) => {
    const buildForm = () => {
      const form = new FormData();
      if (folderId !== undefined && folderId !== null) {
        form.append("folderId", String(folderId));
      }
      for (const f of files) {
        form.append("files", f);
      }
      return form;
    };

    const doUpload = async (token: string | null) =>
      fetch(`${API_BASE}/admin/media/upload`, {
        method: "POST",
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: buildForm(),
      });

    let res = await doUpload(useAuthStore.getState().token);
    if (res.status === 401) {
      const refreshed = await tryRefreshSession();
      if (refreshed) {
        res = await doUpload(refreshed);
      }
    }
    if (!res.ok) {
      let msg = `HTTP ${res.status}`;
      try {
        const errBody = (await res.json()) as ApiErrorEnvelope;
        if (errBody?.error?.message) msg = errBody.error.message;
      } catch {
        // fallback
      }
      throw new Error(msg);
    }
    const json = (await res.json()) as ApiResponse<MediaFileDto[]>;
    return json.data;
  },

  updateMediaFile: async (
    id: number,
    payload: {
      name?: string;
      alternativeText?: string;
      caption?: string;
      folderId?: number;
      moveToRoot?: boolean;
    },
  ) =>
    (
      await requestJson<ApiResponse<MediaFileDto>>(`/admin/media/files/${id}`, {
        method: "PUT",
        body: JSON.stringify(payload),
      })
    ).data,

  cropMediaFile: async (
    id: number,
    payload: {
      x: number;
      y: number;
      width: number;
      height: number;
      resizeWidth?: number;
      resizeHeight?: number;
    },
  ) =>
    (
      await requestJson<ApiResponse<MediaFileDto>>(`/admin/media/files/${id}/crop`, {
        method: "POST",
        body: JSON.stringify(payload),
      })
    ).data,

  deleteMediaFile: async (id: number, force = false) =>
    requestJson<ApiResponse<MediaFileDto>>(
      `/admin/media/files/${id}${force ? "?force=true" : ""}`,
      { method: "DELETE" },
    ),

  listMediaFolders: async () =>
    (await requestJson<ApiResponse<MediaFolderDto[]>>("/admin/media/folders")).data,

  createMediaFolder: async (name: string, parentId?: number | null) =>
    (
      await requestJson<ApiResponse<MediaFolderDto>>("/admin/media/folders", {
        method: "POST",
        body: JSON.stringify({ name, parentId: parentId ?? null }),
      })
    ).data,

  renameMediaFolder: async (id: number, name: string) =>
    (
      await requestJson<ApiResponse<MediaFolderDto>>(`/admin/media/folders/${id}`, {
        method: "PUT",
        body: JSON.stringify({ name }),
      })
    ).data,

  deleteMediaFolder: async (id: number) =>
    requestJson<ApiResponse<{ deleted: number }>>(`/admin/media/folders/${id}`, {
      method: "DELETE",
    }),
};
