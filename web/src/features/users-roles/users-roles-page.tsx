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
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  useToast,
} from "@/components/ui";
import { type AdminUserDto, api, type PermissionCatalogItem } from "@/lib/api";

export function UsersRolesPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const usersQuery = useQuery({ queryKey: ["admin-users"], queryFn: api.listAdminUsers });
  const rolesQuery = useQuery({ queryKey: ["admin-roles"], queryFn: api.listRoles });
  const catalogQuery = useQuery({
    queryKey: ["permission-catalog"],
    queryFn: api.getPermissionCatalog,
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
          <TabsTrigger value="roles">Roles &amp; Permission Matrix</TabsTrigger>
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
                  <Badge tone={u.isActive ? "emerald" : "rose"}>
                    {u.isActive ? "Active" : "Inactive"}
                  </Badge>
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
      </Tabs>
    </div>
  );
}
