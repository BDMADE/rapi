import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Save, Shield, Trash2, UserPlus } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Checkbox,
  DataTable,
  Dialog,
  DialogContent,
  DialogTrigger,
  Input,
  Select,
  Switch,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  useToast,
} from "@/components/ui";
import {
  type AdminUserDto,
  api,
  type EndUserDto,
  type PermissionCatalogItem,
} from "@/lib/api";

export function UsersRolesPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const usersQuery = useQuery({ queryKey: ["admin-users"], queryFn: api.listAdminUsers });
  const rolesQuery = useQuery({ queryKey: ["admin-roles"], queryFn: api.listRoles });
  const endUsersQuery = useQuery({ queryKey: ["end-users"], queryFn: api.listEndUsers });
  const userRolesQuery = useQuery({ queryKey: ["user-roles"], queryFn: api.listUserRoles });
  const catalogQuery = useQuery({
    queryKey: ["permission-catalog"],
    queryFn: api.getPermissionCatalog,
  });
  const schemasQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });
  const [selectedUserRoleId, setSelectedUserRoleId] = useState<string>("urole_public");
  const publicPermsQuery = useQuery({
    queryKey: ["user-role-permissions", selectedUserRoleId],
    queryFn: () => api.getUserRolePermissions(selectedUserRoleId),
  });

  const [publicActions, setPublicActions] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (publicPermsQuery.data) {
      setPublicActions(new Set(publicPermsQuery.data.map((r) => r.action)));
    }
  }, [publicPermsQuery.data]);

  const savePublicPermsMut = useMutation({
    mutationFn: () =>
      api.updateUserRolePermissions(
        selectedUserRoleId,
        Array.from(publicActions).map((action) => ({ action })),
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["user-role-permissions", selectedUserRoleId] });
      queryClient.invalidateQueries({ queryKey: ["public-permissions"] });
      pushToast({ title: "Content API & GraphQL role permissions saved", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Save failed", description: e.message, tone: "error" }),
  });

  // End-user create modal state
  const [endUserOpen, setEndUserOpen] = useState(false);
  const [euUsername, setEuUsername] = useState("");
  const [euEmail, setEuEmail] = useState("");
  const [euPassword, setEuPassword] = useState("");

  const createEndUserMut = useMutation({
    mutationFn: () =>
      api.createEndUser({
        username: euUsername.trim(),
        email: euEmail.trim(),
        password: euPassword,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["end-users"] });
      queryClient.invalidateQueries({ queryKey: ["user-roles"] });
      setEndUserOpen(false);
      setEuUsername("");
      setEuEmail("");
      setEuPassword("");
      pushToast({ title: "End-user registered", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Create failed", description: e.message, tone: "error" }),
  });

  const toggleEndUserBlockedMut = useMutation({
    mutationFn: ({ id, blocked }: { id: string; blocked: boolean }) =>
      api.updateEndUser(id, { blocked }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["end-users"] });
      pushToast({ title: "Updated end-user block status", tone: "success" });
    },
  });

  const deleteEndUserMut = useMutation({
    mutationFn: (id: string) => api.deleteEndUser(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["end-users"] });
      queryClient.invalidateQueries({ queryKey: ["user-roles"] });
      pushToast({ title: "End-user deleted", tone: "success" });
    },
  });

  const [selectedRoleId, setSelectedRoleId] = useState<string>("role_editor");
  const rolePermsQuery = useQuery({
    queryKey: ["role-permissions", selectedRoleId],
    queryFn: () => api.getRolePermissions(selectedRoleId),
    enabled: Boolean(selectedRoleId),
  });

  const [checkedActions, setCheckedActions] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (rolePermsQuery.data) {
      setCheckedActions(new Set(rolePermsQuery.data.map((r) => r.action)));
    }
  }, [rolePermsQuery.data]);

  // Create user modal state
  const [userOpen, setUserOpen] = useState(false);
  const [uEmail, setUEmail] = useState("");
  const [uFirst, setUFirst] = useState("");
  const [uLast, setULast] = useState("");
  const [uPass, setUPass] = useState("");
  const [uRoleId, setURoleId] = useState("role_author");

  // Create role modal state
  const [roleOpen, setRoleOpen] = useState(false);
  const [rName, setRName] = useState("");
  const [rDesc, setRDesc] = useState("");

  const createUserMut = useMutation({
    mutationFn: api.createAdminUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin-users"] });
      queryClient.invalidateQueries({ queryKey: ["admin-roles"] });
      setUserOpen(false);
      setUEmail("");
      setUFirst("");
      setULast("");
      setUPass("");
      pushToast({ title: "Administrator created", tone: "success" });
    },
    onError: (e: Error) => pushToast({ title: "Failed to create user", description: e.message, tone: "error" }),
  });

  const toggleActiveMut = useMutation({
    mutationFn: ({ id, isActive }: { id: string; isActive: boolean }) =>
      api.updateAdminUser(id, { isActive }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin-users"] });
      pushToast({ title: "Administrator status updated", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Update failed", description: e.message, tone: "error" }),
  });

  const deleteUserMut = useMutation({
    mutationFn: api.deleteAdminUser,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin-users"] });
      queryClient.invalidateQueries({ queryKey: ["admin-roles"] });
      pushToast({ title: "Administrator deleted", tone: "success" });
    },
    onError: (e: Error) => pushToast({ title: "Delete failed", description: e.message, tone: "error" }),
  });

  const createRoleMut = useMutation({
    mutationFn: api.createRole,
    onSuccess: (role) => {
      queryClient.invalidateQueries({ queryKey: ["admin-roles"] });
      setSelectedRoleId(role.id);
      setRoleOpen(false);
      setRName("");
      setRDesc("");
      pushToast({ title: `Role "${role.name}" created`, tone: "success" });
    },
    onError: (e: Error) => pushToast({ title: "Failed to create role", description: e.message, tone: "error" }),
  });

  const deleteRoleMut = useMutation({
    mutationFn: api.deleteRole,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin-roles"] });
      setSelectedRoleId("role_editor");
      pushToast({ title: "Role deleted", tone: "success" });
    },
    onError: (e: Error) => pushToast({ title: "Delete failed", description: e.message, tone: "error" }),
  });

  const savePermsMut = useMutation({
    mutationFn: () =>
      api.updateRolePermissions(
        selectedRoleId,
        Array.from(checkedActions).map((action) => ({ action })),
      ),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["role-permissions", selectedRoleId] });
      pushToast({ title: "Role permissions saved", tone: "success" });
    },
    onError: (e: Error) => pushToast({ title: "Save failed", description: e.message, tone: "error" }),
  });

  const groupedCatalog = useMemo(() => {
    const groups = new Map<string, PermissionCatalogItem[]>();
    for (const item of catalogQuery.data ?? []) {
      const list = groups.get(item.category) ?? [];
      list.push(item);
      groups.set(item.category, list);
    }
    return Array.from(groups.entries());
  }, [catalogQuery.data]);

  const activeRole = rolesQuery.data?.find((r) => r.id === selectedRoleId);
  const isSuperAdminRole = selectedRoleId === "role_super_admin";

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            Users, Roles &amp; RBAC
          </h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Manage administrator accounts and enforce granular role permissions across every endpoint.
          </p>
        </div>
      </div>

      <Tabs defaultValue="users" className="space-y-4">
        <TabsList>
          <TabsTrigger value="users">Administrators ({usersQuery.data?.length ?? 0})</TabsTrigger>
          <TabsTrigger value="end-users">
            End-Users ({endUsersQuery.data?.length ?? 0})
          </TabsTrigger>
          <TabsTrigger value="roles">Admin RBAC Matrix</TabsTrigger>
          <TabsTrigger value="public">Content API &amp; GraphQL Roles</TabsTrigger>
        </TabsList>

        <TabsContent value="users" className="space-y-3">
          <div className="flex justify-end">
            <Dialog open={userOpen} onOpenChange={setUserOpen}>
              <DialogTrigger asChild>
                <Button size="sm">
                  <UserPlus className="h-3.5 w-3.5" />
                  Add administrator
                </Button>
              </DialogTrigger>
              <DialogContent
                title="Create administrator"
                description="Account credentials are hashed with Argon2id."
              >
                <form
                  className="space-y-3"
                  onSubmit={(e) => {
                    e.preventDefault();
                    createUserMut.mutate({
                      email: uEmail,
                      password: uPass,
                      firstname: uFirst,
                      lastname: uLast,
                      roleIds: [uRoleId],
                    });
                  }}
                >
                  <div className="grid grid-cols-2 gap-2.5">
                    <div>
                      <label className="mb-1 block text-xs font-medium text-zinc-700">
                        First name
                      </label>
                      <Input required value={uFirst} onChange={(e) => setUFirst(e.target.value)} />
                    </div>
                    <div>
                      <label className="mb-1 block text-xs font-medium text-zinc-700">
                        Last name
                      </label>
                      <Input value={uLast} onChange={(e) => setULast(e.target.value)} />
                    </div>
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-zinc-700">Email</label>
                    <Input
                      required
                      type="email"
                      value={uEmail}
                      onChange={(e) => setUEmail(e.target.value)}
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-zinc-700">
                      Password (min 8 chars)
                    </label>
                    <Input
                      required
                      minLength={8}
                      type="password"
                      value={uPass}
                      onChange={(e) => setUPass(e.target.value)}
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-xs font-medium text-zinc-700">Role</label>
                    <Select
                      value={uRoleId}
                      onValueChange={setURoleId}
                      options={(rolesQuery.data ?? []).map((r) => ({
                        value: r.id,
                        label: r.name,
                      }))}
                    />
                  </div>
                  <div className="flex justify-end gap-2 pt-2">
                    <Button type="button" variant="secondary" onClick={() => setUserOpen(false)}>
                      Cancel
                    </Button>
                    <Button type="submit" disabled={createUserMut.isPending}>
                      Create user
                    </Button>
                  </div>
                </form>
              </DialogContent>
            </Dialog>
          </div>

          <DataTable<AdminUserDto>
            rows={usersQuery.data ?? []}
            getRowKey={(u) => u.id}
            emptyMessage="No administrators found."
            columns={[
              {
                key: "name",
                header: "Name",
                render: (u) => (
                  <span className="font-medium text-zinc-900">
                    {u.firstname} {u.lastname}
                  </span>
                ),
              },
              {
                key: "email",
                header: "Email",
                render: (u) => <span className="font-mono text-[11px]">{u.email}</span>,
              },
              {
                key: "roles",
                header: "Roles",
                render: (u) => (
                  <div className="flex flex-wrap gap-1">
                    {u.roles.map((r) => (
                      <Badge key={r.id} tone={r.id === "role_super_admin" ? "indigo" : "neutral"}>
                        {r.name}
                      </Badge>
                    ))}
                  </div>
                ),
              },
              {
                key: "status",
                header: "Status",
                render: (u) => (
                  <div className="flex items-center gap-2">
                    <Switch
                      checked={u.isActive}
                      onCheckedChange={(checked) =>
                        toggleActiveMut.mutate({ id: u.id, isActive: checked })
                      }
                      disabled={toggleActiveMut.isPending}
                    />
                    <Badge tone={u.isActive ? "emerald" : "rose"}>
                      {u.isActive ? "Active" : "Inactive"}
                    </Badge>
                  </div>
                ),
              },
              {
                key: "actions",
                header: "",
                className: "text-right",
                render: (u) => (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => deleteUserMut.mutate(u.id)}
                    disabled={deleteUserMut.isPending}
                  >
                    <Trash2 className="h-3.5 w-3.5 text-zinc-500 hover:text-rose-600" />
                  </Button>
                ),
              },
            ]}
          />
        </TabsContent>

        <TabsContent value="roles">
          <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
            {/* Role list */}
            <Card className="lg:col-span-4">
              <CardHeader>
                <CardTitle>Admin roles</CardTitle>
                <Dialog open={roleOpen} onOpenChange={setRoleOpen}>
                  <DialogTrigger asChild>
                    <Button size="sm" variant="secondary">
                      <Plus className="h-3.5 w-3.5" />
                      New role
                    </Button>
                  </DialogTrigger>
                  <DialogContent title="Create admin role">
                    <form
                      className="space-y-3"
                      onSubmit={(e) => {
                        e.preventDefault();
                        createRoleMut.mutate({ name: rName, description: rDesc });
                      }}
                    >
                      <div>
                        <label className="mb-1 block text-xs font-medium text-zinc-700">
                          Role name
                        </label>
                        <Input
                          required
                          placeholder="Content Reviewer"
                          value={rName}
                          onChange={(e) => setRName(e.target.value)}
                        />
                      </div>
                      <div>
                        <label className="mb-1 block text-xs font-medium text-zinc-700">
                          Description
                        </label>
                        <Input
                          placeholder="Can review drafts and publish content"
                          value={rDesc}
                          onChange={(e) => setRDesc(e.target.value)}
                        />
                      </div>
                      <div className="flex justify-end gap-2 pt-2">
                        <Button
                          type="button"
                          variant="secondary"
                          onClick={() => setRoleOpen(false)}
                        >
                          Cancel
                        </Button>
                        <Button type="submit" disabled={createRoleMut.isPending}>
                          Create role
                        </Button>
                      </div>
                    </form>
                  </DialogContent>
                </Dialog>
              </CardHeader>
              <div className="divide-y divide-zinc-200">
                {(rolesQuery.data ?? []).map((role) => (
                  <div
                    key={role.id}
                    onClick={() => setSelectedRoleId(role.id)}
                    className={`flex cursor-pointer items-center justify-between px-4 py-3 text-xs transition-colors ${
                      selectedRoleId === role.id
                        ? "bg-indigo-50/70 text-indigo-950"
                        : "hover:bg-zinc-50"
                    }`}
                  >
                    <div>
                      <div className="font-medium text-zinc-900">{role.name}</div>
                      <div className="mt-0.5 text-[11px] text-zinc-500">
                        {role.userCount} user{role.userCount === 1 ? "" : "s"} ·{" "}
                        <span className="font-mono">{role.code}</span>
                      </div>
                    </div>
                    {!["role_super_admin", "role_editor", "role_author"].includes(role.id) && (
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteRoleMut.mutate(role.id);
                        }}
                      >
                        <Trash2 className="h-3.5 w-3.5 text-zinc-400 hover:text-rose-600" />
                      </Button>
                    )}
                  </div>
                ))}
              </div>
            </Card>

            {/* Permission matrix */}
            <Card className="lg:col-span-8">
              <CardHeader>
                <div className="flex items-center gap-2">
                  <Shield className="h-4 w-4 text-indigo-700" />
                  <CardTitle>
                    Permissions: {activeRole?.name ?? selectedRoleId}
                  </CardTitle>
                </div>
                {!isSuperAdminRole && (
                  <Button
                    size="sm"
                    onClick={() => savePermsMut.mutate()}
                    disabled={savePermsMut.isPending}
                  >
                    <Save className="h-3.5 w-3.5" />
                    Save permissions
                  </Button>
                )}
              </CardHeader>
              <CardContent className="space-y-5">
                {isSuperAdminRole ? (
                  <p className="rounded border border-indigo-200 bg-indigo-50 p-3 text-xs text-indigo-950">
                    Super Admin implicitly holds unrestricted access to all platform and content
                    actions. Select Editor, Author, or a custom role to configure granular rules.
                  </p>
                ) : (
                  groupedCatalog.map(([category, items]) => (
                    <div key={category} className="space-y-2">
                      <h4 className="text-xs font-semibold text-zinc-900">{category}</h4>
                      <div className="divide-y divide-zinc-200 rounded border border-zinc-200">
                        {items.map((item) => {
                          const checked = checkedActions.has(item.action);
                          return (
                            <label
                              key={item.action}
                              className="flex cursor-pointer items-start gap-3 px-3 py-2.5 hover:bg-zinc-50"
                            >
                              <Checkbox
                                checked={checked}
                                onCheckedChange={(val) => {
                                  const next = new Set(checkedActions);
                                  if (val) next.add(item.action);
                                  else next.delete(item.action);
                                  setCheckedActions(next);
                                }}
                                className="mt-0.5"
                              />
                              <div className="flex-1">
                                <div className="flex items-center justify-between gap-2">
                                  <span className="font-medium text-zinc-900">{item.label}</span>
                                  <span className="font-mono text-[10px] text-zinc-400">
                                    {item.action}
                                  </span>
                                </div>
                                <p className="mt-0.5 text-[11px] text-zinc-500">
                                  {item.description}
                                </p>
                              </div>
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  ))
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="end-users" className="space-y-3">
          <div className="flex justify-end">
            <Dialog open={endUserOpen} onOpenChange={setEndUserOpen}>
              <DialogTrigger asChild>
                <Button size="sm">
                  <Plus className="h-3.5 w-3.5" />
                  Register end-user
                </Button>
              </DialogTrigger>
              <DialogContent title="Create End-User (/api/auth/local)">
                <div className="space-y-3 text-xs">
                  <div>
                    <label className="mb-1 block font-medium text-zinc-700">Username</label>
                    <Input
                      value={euUsername}
                      onChange={(e) => setEuUsername(e.target.value)}
                      placeholder="alice_dev"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block font-medium text-zinc-700">Email</label>
                    <Input
                      type="email"
                      value={euEmail}
                      onChange={(e) => setEuEmail(e.target.value)}
                      placeholder="alice@example.com"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block font-medium text-zinc-700">
                      Password (min 8 chars)
                    </label>
                    <Input
                      type="password"
                      value={euPassword}
                      onChange={(e) => setEuPassword(e.target.value)}
                    />
                  </div>
                  <div className="flex justify-end gap-2 pt-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => setEndUserOpen(false)}
                    >
                      Cancel
                    </Button>
                    <Button
                      size="sm"
                      disabled={
                        !euUsername.trim() ||
                        !euEmail.trim() ||
                        euPassword.length < 8 ||
                        createEndUserMut.isPending
                      }
                      onClick={() => createEndUserMut.mutate()}
                    >
                      Create end-user
                    </Button>
                  </div>
                </div>
              </DialogContent>
            </Dialog>
          </div>

          <DataTable<EndUserDto>
            rows={endUsersQuery.data ?? []}
            getRowKey={(u) => u.id}
            emptyMessage="No registered end-users yet."
            columns={[
              {
                key: "username",
                header: "Username",
                render: (u) => <span className="font-medium text-zinc-900">{u.username}</span>,
              },
              {
                key: "email",
                header: "Email",
                render: (u) => <span className="font-mono text-xs">{u.email}</span>,
              },
              {
                key: "role",
                header: "Role",
                render: (u) => <Badge tone="mono">{u.roleName}</Badge>,
              },
              {
                key: "blocked",
                header: "Status",
                render: (u) => (
                  <div className="flex items-center gap-2">
                    <Switch
                      checked={!u.blocked}
                      onCheckedChange={(active) =>
                        toggleEndUserBlockedMut.mutate({ id: u.id, blocked: !active })
                      }
                    />
                    <Badge tone={u.blocked ? "rose" : "emerald"}>
                      {u.blocked ? "blocked" : "active"}
                    </Badge>
                  </div>
                ),
              },
              {
                key: "actions",
                header: "",
                className: "text-right",
                render: (u) => (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      if (window.confirm(`Delete end-user "${u.username}"?`)) {
                        deleteEndUserMut.mutate(u.id);
                      }
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5 text-rose-600" />
                  </Button>
                ),
              },
            ]}
          />
        </TabsContent>

        <TabsContent value="public">
          <Card>
            <CardHeader className="flex flex-row flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                <CardTitle>REST &amp; GraphQL Role Permissions</CardTitle>
                <div className="w-48">
                  <Select
                    value={selectedUserRoleId}
                    onValueChange={setSelectedUserRoleId}
                    options={
                      (userRolesQuery.data ?? []).length > 0
                        ? (userRolesQuery.data ?? []).map((r) => ({
                            value: r.id,
                            label: `${r.name} (${r.roleType})`,
                          }))
                        : [
                            { value: "urole_public", label: "Public (public)" },
                            { value: "urole_authenticated", label: "Authenticated (authenticated)" },
                          ]
                    }
                  />
                </div>
              </div>
              <Button
                size="sm"
                onClick={() => savePublicPermsMut.mutate()}
                disabled={savePublicPermsMut.isPending}
              >
                <Save className="h-3.5 w-3.5" />
                Save role permissions
              </Button>
            </CardHeader>
            <CardContent className="space-y-4">
              {(schemasQuery.data?.contentTypes ?? []).length === 0 ? (
                <p className="text-xs text-zinc-500">
                  Create a content type first to configure Public REST API permissions.
                </p>
              ) : (
                (schemasQuery.data?.contentTypes ?? []).map((ct) => {
                  const ops = ["find", "findOne", "create", "update", "delete"];
                  return (
                    <div
                      key={ct.uid}
                      className="flex flex-wrap items-center justify-between gap-3 rounded border border-zinc-200 p-3"
                    >
                      <div>
                        <div className="text-xs font-semibold text-zinc-900">{ct.displayName}</div>
                        <div className="font-mono text-[11px] text-zinc-500">{ct.uid}</div>
                      </div>
                      <div className="flex flex-wrap items-center gap-4 text-xs">
                        {ops.map((op) => {
                          const act = `${ct.uid}.${op}`;
                          return (
                            <label key={act} className="flex cursor-pointer items-center gap-1.5">
                              <Checkbox
                                checked={publicActions.has(act)}
                                onCheckedChange={(val) => {
                                  const next = new Set(publicActions);
                                  if (val) next.add(act);
                                  else next.delete(act);
                                  setPublicActions(next);
                                }}
                              />
                              <span className="font-mono">{op}</span>
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  );
                })
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
