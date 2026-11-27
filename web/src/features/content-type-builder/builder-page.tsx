import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  Blocks,
  Code2,
  Database,
  Layers,
  Pencil,
  Plus,
  Save,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  Checkbox,
  DataTable,
  Dialog,
  DialogContent,
  EmptyState,
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
  api,
  type ContentKind,
  type ContentTypeSchema,
  type FieldDefinition,
  type FieldType,
  type RelationKind,
} from "@/lib/api";

const FIELD_TYPES: { value: FieldType; label: string; group: string }[] = [
  { value: "string", label: "Short text (string)", group: "Text" },
  { value: "text", label: "Long text (text)", group: "Text" },
  { value: "richtext", label: "Rich text (Markdown)", group: "Rich Content" },
  { value: "blocks", label: "Blocks (Strapi v5 JSON AST)", group: "Rich Content" },
  { value: "integer", label: "Integer", group: "Numbers" },
  { value: "biginteger", label: "Big integer", group: "Numbers" },
  { value: "float", label: "Float", group: "Numbers" },
  { value: "decimal", label: "Decimal", group: "Numbers" },
  { value: "boolean", label: "Boolean", group: "Scalar" },
  { value: "date", label: "Date", group: "Date & Time" },
  { value: "datetime", label: "DateTime", group: "Date & Time" },
  { value: "time", label: "Time", group: "Date & Time" },
  { value: "email", label: "Email", group: "Text" },
  { value: "password", label: "Password", group: "Text" },
  { value: "uid", label: "UID (slug)", group: "Text" },
  { value: "enumeration", label: "Enumeration", group: "Scalar" },
  { value: "json", label: "JSON", group: "Structured" },
  { value: "media", label: "Media asset", group: "References" },
  { value: "relation", label: "Relation", group: "References" },
  { value: "component", label: "Component", group: "Structured" },
  { value: "dynamiczone", label: "Dynamic Zone", group: "Structured" },
  { value: "custom", label: "Custom Field", group: "Structured" },
];

const RELATION_TYPES: { value: RelationKind; label: string }[] = [
  { value: "manyToOne", label: "Many-to-One (belongs to)" },
  { value: "oneToMany", label: "One-to-Many (has many)" },
  { value: "manyToMany", label: "Many-to-Many" },
  { value: "oneToOne", label: "One-to-One" },
  { value: "morphOne", label: "Polymorphic (morphOne)" },
  { value: "morphMany", label: "Polymorphic (morphMany)" },
];

function slugify(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function pluralizeSlug(singular: string): string {
  if (!singular) return "";
  if (singular.endsWith("y") && !/[aeiou]y$/.test(singular)) {
    return `${singular.slice(0, -1)}ies`;
  }
  if (singular.endsWith("s") || singular.endsWith("x") || singular.endsWith("ch")) {
    return `${singular}es`;
  }
  return `${singular}s`;
}

export function ContentTypeBuilderPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();

  const catalogQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });

  const allSchemas = useMemo(() => {
    if (!catalogQuery.data) return [];
    return [...catalogQuery.data.contentTypes, ...catalogQuery.data.components];
  }, [catalogQuery.data]);

  const [selectedUid, setSelectedUid] = useState<string | null>(null);
  const [draft, setDraft] = useState<ContentTypeSchema | null>(null);
  const [dirty, setDirty] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);

  const selectSchemaSafely = (uid: string) => {
    if (uid === selectedUid) return;
    if (dirty && !window.confirm("Discard unsaved field/schema changes?")) {
      return;
    }
    setSelectedUid(uid);
  };

  // Select first available schema when catalog loads
  useEffect(() => {
    if (!selectedUid && allSchemas.length > 0) {
      setSelectedUid(allSchemas[0].uid);
    } else if (selectedUid && allSchemas.length > 0 && !allSchemas.some((s) => s.uid === selectedUid)) {
      setSelectedUid(allSchemas[0]?.uid ?? null);
    }
  }, [allSchemas, selectedUid]);

  useEffect(() => {
    const found = allSchemas.find((s) => s.uid === selectedUid) ?? null;
    setDraft(found ? structuredClone(found) : null);
    setDirty(false);
  }, [selectedUid, allSchemas]);

  const inspectionQuery = useQuery({
    queryKey: ["schema-inspection", selectedUid],
    queryFn: () => api.inspectSchema(selectedUid!),
    enabled: Boolean(selectedUid),
  });

  // --- Create Schema Dialog state ---
  const [createOpen, setCreateOpen] = useState(false);
  const [newKind, setNewKind] = useState<ContentKind>("collectionType");
  const [newDisplayName, setNewDisplayName] = useState("");
  const [newSingular, setNewSingular] = useState("");
  const [newPlural, setNewPlural] = useState("");
  const [newCategory, setNewCategory] = useState("shared");
  const [newDraftPub, setNewDraftPub] = useState(true);
  const [newI18n, setNewI18n] = useState(false);

  const handleDisplayNameChange = (val: string) => {
    setNewDisplayName(val);
    const sing = slugify(val);
    setNewSingular(sing);
    setNewPlural(newKind === "collectionType" ? pluralizeSlug(sing) : `${sing}-single`);
  };

  const computedNewUid =
    newKind === "component"
      ? `${slugify(newCategory) || "shared"}.${newSingular || "item"}`
      : `api::${newSingular || "item"}.${newSingular || "item"}`;

  const createSchemaMut = useMutation({
    mutationFn: () =>
      api.createSchema({
        uid: computedNewUid,
        kind: newKind,
        category: newKind === "component" ? slugify(newCategory) || "shared" : undefined,
        singularName: newSingular,
        pluralName: newKind === "collectionType" ? newPlural : `${newSingular}-single`,
        displayName: newDisplayName,
        description: "",
        icon: newKind === "component" ? "layers" : "file-text",
        draftAndPublish: newKind === "component" ? false : newDraftPub,
        i18nEnabled: newKind === "component" ? false : newI18n,
        fields: [],
      }),
    onSuccess: (created) => {
      queryClient.invalidateQueries({ queryKey: ["schemas-catalog"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      setSelectedUid(created.uid);
      setCreateOpen(false);
      setNewDisplayName("");
      setNewSingular("");
      setNewPlural("");
      pushToast({
        title: `Created ${created.displayName}`,
        description:
          created.kind === "component"
            ? `Component ${created.uid} registered.`
            : `SQLite table generated with zero restart.`,
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "Schema creation failed", description: e.message, tone: "error" }),
  });

  const saveSchemaMut = useMutation({
    mutationFn: () => api.updateSchema(draft!.uid, draft!),
    onSuccess: (updated) => {
      queryClient.invalidateQueries({ queryKey: ["schemas-catalog"] });
      queryClient.invalidateQueries({ queryKey: ["schema-inspection", updated.uid] });
      setDirty(false);
      pushToast({
        title: "Schema updated",
        description: "SQLite DDL & FTS5 indexes synchronized.",
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "Schema update failed", description: e.message, tone: "error" }),
  });

  const deleteSchemaMut = useMutation({
    mutationFn: (uid: string) => api.deleteSchema(uid),
    onSuccess: () => {
      setSelectedUid(null);
      queryClient.invalidateQueries({ queryKey: ["schemas-catalog"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Schema deleted", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Cannot delete schema", description: e.message, tone: "error" }),
  });

  // --- Field Modal State ---
  const [fieldOpen, setFieldOpen] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [fName, setFName] = useState("");
  const [fType, setFType] = useState<FieldType>("string");
  const [fRequired, setFRequired] = useState(false);
  const [fUnique, setFUnique] = useState(false);
  const [fLocalized, setFLocalized] = useState(false);
  const [fPrivate, setFPrivate] = useState(false);
  const [fEnumRaw, setFEnumRaw] = useState("draft, review, featured");
  const [fTargetField, setFTargetField] = useState("");
  const [fMediaMultiple, setFMediaMultiple] = useState(false);
  const [fMinLen, setFMinLen] = useState("");
  const [fMaxLen, setFMaxLen] = useState("");
  const [fRegex, setFRegex] = useState("");
  const [fRelKind, setFRelKind] = useState<RelationKind>("manyToOne");
  const [fRelTarget, setFRelTarget] = useState("");
  const [fCompUid, setFCompUid] = useState("");
  const [fCompRepeatable, setFCompRepeatable] = useState(false);
  const [fDzComponents, setFDzComponents] = useState<Set<string>>(new Set());

  const openAddField = () => {
    setEditingIndex(null);
    setFName("");
    setFType("string");
    setFRequired(false);
    setFUnique(false);
    setFLocalized(Boolean(draft?.i18nEnabled));
    setFPrivate(false);
    setFEnumRaw("news, tutorial");
    setFTargetField("");
    setFMediaMultiple(false);
    setFMinLen("");
    setFMaxLen("");
    setFRegex("");
    setFRelKind("manyToOne");
    setFRelTarget(catalogQuery.data?.contentTypes[0]?.uid ?? draft?.uid ?? "");
    setFCompUid(catalogQuery.data?.components[0]?.uid ?? "");
    setFCompRepeatable(false);
    setFDzComponents(new Set(catalogQuery.data?.components.slice(0, 1).map((c) => c.uid) ?? []));
    setFieldOpen(true);
  };

  const openEditField = (idx: number, f: FieldDefinition) => {
    setEditingIndex(idx);
    setFName(f.name);
    setFType(f.fieldType);
    setFRequired(Boolean(f.required));
    setFUnique(Boolean(f.unique));
    setFLocalized(Boolean(f.localized));
    setFPrivate(Boolean(f.private));
    setFEnumRaw((f.validation?.enumValues ?? []).join(", "));
    setFTargetField(f.validation?.targetField ?? "");
    setFMediaMultiple(Boolean(f.validation?.multiple));
    setFMinLen(f.validation?.minLength !== undefined ? String(f.validation.minLength) : "");
    setFMaxLen(f.validation?.maxLength !== undefined ? String(f.validation.maxLength) : "");
    setFRegex(f.validation?.regex ?? "");
    setFRelKind(f.relation?.relationType ?? "manyToOne");
    setFRelTarget(f.relation?.targetUid ?? "");
    setFCompUid(f.component?.componentUid ?? "");
    setFCompRepeatable(Boolean(f.component?.repeatable));
    setFDzComponents(new Set(f.component?.components ?? []));
    setFieldOpen(true);
  };

  const commitFieldModal = () => {
    if (!draft) return;
    const trimmedName = fName.trim();
    if (!/^[a-zA-Z][a-zA-Z0-9_]{0,63}$/.test(trimmedName)) {
      pushToast({
        title: "Invalid field name",
        description: "Use 1–64 ASCII letters, digits, or underscores starting with a letter.",
        tone: "error",
      });
      return;
    }
    const isDuplicate = draft.fields.some(
      (f, idx) => idx !== editingIndex && f.name.toLowerCase() === trimmedName.toLowerCase(),
    );
    if (isDuplicate) {
      pushToast({
        title: `Field "${trimmedName}" already exists`,
        description: "Field names must be unique within a schema.",
        tone: "error",
      });
      return;
    }
    const built: FieldDefinition = {
      name: trimmedName,
      fieldType: fType,
      required: fRequired,
      unique: fUnique,
      localized: draft.i18nEnabled ? fLocalized : false,
      private: fPrivate,
      validation: {
        minLength: fMinLen ? Number(fMinLen) : undefined,
        maxLength: fMaxLen ? Number(fMaxLen) : undefined,
        regex: fRegex.trim() || undefined,
        enumValues:
          fType === "enumeration"
            ? fEnumRaw
                .split(/[,\n]/)
                .map((s) => s.trim())
                .filter(Boolean)
            : undefined,
        targetField: fType === "uid" && fTargetField ? fTargetField : undefined,
        multiple: fType === "media" ? fMediaMultiple : undefined,
      },
      relation:
        fType === "relation"
          ? {
              relationType: fRelKind,
              targetUid:
                fRelKind === "morphOne" || fRelKind === "morphMany" ? undefined : fRelTarget,
            }
          : undefined,
      component:
        fType === "component"
          ? { componentUid: fCompUid, repeatable: fCompRepeatable }
          : fType === "dynamiczone"
            ? { components: Array.from(fDzComponents) }
            : undefined,
    };

    const nextFields = [...draft.fields];
    if (editingIndex !== null) {
      nextFields[editingIndex] = built;
    } else {
      nextFields.push(built);
    }
    setDraft({ ...draft, fields: nextFields });
    setDirty(true);
    setFieldOpen(false);
  };

  const moveField = (idx: number, dir: -1 | 1) => {
    if (!draft) return;
    const target = idx + dir;
    if (target < 0 || target >= draft.fields.length) return;
    const copy = [...draft.fields];
    const [removed] = copy.splice(idx, 1);
    copy.splice(target, 0, removed);
    setDraft({ ...draft, fields: copy });
    setDirty(true);
  };

  const removeField = (idx: number) => {
    if (!draft) return;
    const copy = draft.fields.filter((_, i) => i !== idx);
    setDraft({ ...draft, fields: copy });
    setDirty(true);
  };

  const collectionTypes = (catalogQuery.data?.contentTypes ?? []).filter(
    (s) => s.kind === "collectionType",
  );
  const singleTypes = (catalogQuery.data?.contentTypes ?? []).filter(
    (s) => s.kind === "singleType",
  );
  const components = catalogQuery.data?.components ?? [];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">
            Content-Type Builder
          </h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Model collection types, single types, components, and dynamic zones — runtime{" "}
            <code className="font-mono">sea-query</code> DDL updates SQLite with zero restarts.
          </p>
        </div>

        <Button
          size="sm"
          onClick={() => {
            setNewKind("collectionType");
            setCreateOpen(true);
          }}
        >
          <Plus className="h-3.5 w-3.5" />
          New schema
        </Button>
      </div>

      {/* Create Schema Modal */}
      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent
          title="Create content type or component"
          description="Generates metadata and provisions SQLite tables & FTS5 indexes immediately."
        >
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              createSchemaMut.mutate();
            }}
          >
            <div>
              <label className="mb-1 block text-xs font-medium text-zinc-700">Schema kind</label>
              <Select
                value={newKind}
                onValueChange={(v) => {
                  const k = v as ContentKind;
                  setNewKind(k);
                  if (newSingular) {
                    setNewPlural(k === "collectionType" ? pluralizeSlug(newSingular) : `${newSingular}-single`);
                  }
                }}
                options={[
                  { value: "collectionType", label: "Collection Type (multiple entries)" },
                  { value: "singleType", label: "Single Type (one global document)" },
                  { value: "component", label: "Component (reusable JSON block)" },
                ]}
              />
            </div>

            <div>
              <label className="mb-1 block text-xs font-medium text-zinc-700">Display name</label>
              <Input
                required
                placeholder={newKind === "component" ? "SEO Meta" : "Article"}
                value={newDisplayName}
                onChange={(e) => handleDisplayNameChange(e.target.value)}
              />
            </div>

            {newKind === "component" ? (
              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">Category</label>
                  <Input
                    required
                    placeholder="shared"
                    value={newCategory}
                    onChange={(e) => setNewCategory(e.target.value)}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">Slug</label>
                  <Input
                    required
                    value={newSingular}
                    onChange={(e) => setNewSingular(slugify(e.target.value))}
                  />
                </div>
              </div>
            ) : (
              <div className="grid grid-cols-2 gap-2.5">
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">
                    API ID (singular)
                  </label>
                  <Input
                    required
                    value={newSingular}
                    onChange={(e) => setNewSingular(slugify(e.target.value))}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">
                    API ID (plural)
                  </label>
                  <Input
                    required
                    value={newPlural}
                    onChange={(e) => setNewPlural(slugify(e.target.value))}
                  />
                </div>
              </div>
            )}

            <div className="rounded border border-zinc-200 bg-zinc-50 px-3 py-2 text-[11px] text-zinc-600">
              Generated UID: <code className="font-mono font-semibold text-zinc-900">{computedNewUid}</code>
            </div>

            {newKind !== "component" && (
              <div className="space-y-2 pt-1">
                <label className="flex items-center justify-between text-xs text-zinc-700">
                  <span>Draft &amp; publish system</span>
                  <Switch checked={newDraftPub} onCheckedChange={setNewDraftPub} />
                </label>
                <label className="flex items-center justify-between text-xs text-zinc-700">
                  <span>Internationalization (i18n)</span>
                  <Switch checked={newI18n} onCheckedChange={setNewI18n} />
                </label>
              </div>
            )}

            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setCreateOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={createSchemaMut.isPending}>
                Create schema
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Add / Edit Field Modal */}
      <Dialog open={fieldOpen} onOpenChange={setFieldOpen}>
        <DialogContent
          title={editingIndex !== null ? `Edit field "${fName}"` : "Add field to schema"}
          className="max-w-lg"
        >
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              commitFieldModal();
            }}
          >
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">Field name</label>
                <Input
                  required
                  placeholder="title"
                  value={fName}
                  onChange={(e) => setFName(e.target.value)}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">Field type</label>
                <Select
                  value={fType}
                  onValueChange={(v) => setFType(v as FieldType)}
                  options={FIELD_TYPES.filter(
                    (ft) => !(draft?.kind === "component" && ft.value === "dynamiczone"),
                  ).map((ft) => ({ value: ft.value, label: ft.label }))}
                />
              </div>
            </div>

            {fType === "enumeration" && (
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">
                  Enum values (comma-separated)
                </label>
                <Input
                  required
                  value={fEnumRaw}
                  onChange={(e) => setFEnumRaw(e.target.value)}
                  placeholder="news, tutorial, release"
                />
              </div>
            )}

            {fType === "uid" && (
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">
                  Attached target field (optional)
                </label>
                <Input
                  value={fTargetField}
                  onChange={(e) => setFTargetField(e.target.value)}
                  placeholder="title"
                />
              </div>
            )}

            {fType === "media" && (
              <label className="flex items-center justify-between rounded border border-zinc-200 px-3 py-2 text-xs text-zinc-700">
                <span>Allow multiple media files</span>
                <Switch checked={fMediaMultiple} onCheckedChange={setFMediaMultiple} />
              </label>
            )}

            {fType === "relation" && (
              <div className="grid grid-cols-2 gap-3 rounded border border-zinc-200 bg-zinc-50 p-3">
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">
                    Cardinality
                  </label>
                  <Select
                    value={fRelKind}
                    onValueChange={(v) => setFRelKind(v as RelationKind)}
                    options={RELATION_TYPES}
                  />
                </div>
                {fRelKind !== "morphOne" && fRelKind !== "morphMany" && (
                  <div>
                    <label className="mb-1 block text-xs font-medium text-zinc-700">
                      Target content type
                    </label>
                    <Select
                      value={fRelTarget}
                      onValueChange={setFRelTarget}
                      options={(catalogQuery.data?.contentTypes ?? []).map((ct) => ({
                        value: ct.uid,
                        label: `${ct.displayName} (${ct.uid})`,
                      }))}
                    />
                  </div>
                )}
              </div>
            )}

            {fType === "component" && (
              <div className="space-y-2.5 rounded border border-zinc-200 bg-zinc-50 p-3">
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">
                    Target component
                  </label>
                  <Select
                    value={fCompUid}
                    onValueChange={setFCompUid}
                    options={components
                      .filter((c) => c.uid !== draft?.uid)
                      .map((c) => ({ value: c.uid, label: `${c.displayName} (${c.uid})` }))}
                  />
                </div>
                <label className="flex items-center justify-between text-xs text-zinc-700">
                  <span>Repeatable component array</span>
                  <Switch checked={fCompRepeatable} onCheckedChange={setFCompRepeatable} />
                </label>
              </div>
            )}

            {fType === "dynamiczone" && (
              <div className="space-y-1.5">
                <label className="block text-xs font-medium text-zinc-700">
                  Allowed components ({fDzComponents.size} selected)
                </label>
                <div className="max-h-36 space-y-1 overflow-y-auto rounded border border-zinc-200 p-2">
                  {components.length === 0 ? (
                    <p className="text-xs text-zinc-500">Create a component first.</p>
                  ) : (
                    components.map((c) => (
                      <label
                        key={c.uid}
                        className="flex cursor-pointer items-center gap-2 rounded px-1.5 py-1 text-xs hover:bg-zinc-50"
                      >
                        <Checkbox
                          checked={fDzComponents.has(c.uid)}
                          onCheckedChange={(val) => {
                            const next = new Set(fDzComponents);
                            if (val) next.add(c.uid);
                            else next.delete(c.uid);
                            setFDzComponents(next);
                          }}
                        />
                        <span>{c.displayName}</span>
                        <span className="font-mono text-[10px] text-zinc-400">{c.uid}</span>
                      </label>
                    ))
                  )}
                </div>
              </div>
            )}

            {(fType === "string" || fType === "text") && (
              <div className="grid grid-cols-3 gap-2.5">
                <div>
                  <label className="mb-1 block text-[11px] font-medium text-zinc-600">
                    Min length
                  </label>
                  <Input
                    type="number"
                    min={0}
                    value={fMinLen}
                    onChange={(e) => setFMinLen(e.target.value)}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-[11px] font-medium text-zinc-600">
                    Max length
                  </label>
                  <Input
                    type="number"
                    min={1}
                    value={fMaxLen}
                    onChange={(e) => setFMaxLen(e.target.value)}
                  />
                </div>
                <div>
                  <label className="mb-1 block text-[11px] font-medium text-zinc-600">
                    Regex pattern
                  </label>
                  <Input
                    value={fRegex}
                    onChange={(e) => setFRegex(e.target.value)}
                    placeholder="^[a-z0-9-]+$"
                  />
                </div>
              </div>
            )}

            <div className="grid grid-cols-2 gap-2 border-t border-zinc-200 pt-3 text-xs">
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox
                  checked={fRequired}
                  onCheckedChange={(v) => setFRequired(Boolean(v))}
                />
                <span>Required field</span>
              </label>
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox checked={fUnique} onCheckedChange={(v) => setFUnique(Boolean(v))} />
                <span>Unique constraint</span>
              </label>
              {draft?.i18nEnabled && (
                <label className="flex cursor-pointer items-center gap-2">
                  <Checkbox
                    checked={fLocalized}
                    onCheckedChange={(v) => setFLocalized(Boolean(v))}
                  />
                  <span>Localized per locale</span>
                </label>
              )}
              <label className="flex cursor-pointer items-center gap-2">
                <Checkbox checked={fPrivate} onCheckedChange={(v) => setFPrivate(Boolean(v))} />
                <span>Private (omit from Content API)</span>
              </label>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setFieldOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">
                {editingIndex !== null ? "Update field" : "Add field"}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Main 2-column Builder Grid */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
        {/* Left Catalog Rail */}
        <Card className="lg:col-span-3">
          <div className="divide-y divide-zinc-200">
            <div className="p-3">
              <div className="mb-1.5 flex items-center justify-between text-[11px] font-semibold text-zinc-500">
                <span>Collection Types ({collectionTypes.length})</span>
                <button
                  type="button"
                  onClick={() => {
                    setNewKind("collectionType");
                    setCreateOpen(true);
                  }}
                  className="text-indigo-700 hover:underline cursor-pointer"
                >
                  + Add
                </button>
              </div>
              <div className="space-y-0.5">
                {collectionTypes.map((ct) => (
                  <button
                    key={ct.uid}
                    type="button"
                    onClick={() => selectSchemaSafely(ct.uid)}
                    className={`flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-xs cursor-pointer ${
                      selectedUid === ct.uid
                        ? "bg-indigo-50 font-medium text-indigo-900"
                        : "text-zinc-700 hover:bg-zinc-50"
                    }`}
                  >
                    <span className="truncate">{ct.displayName}</span>
                    <span className="font-mono text-[10px] text-zinc-400">
                      {ct.fields.length}f
                    </span>
                  </button>
                ))}
              </div>
            </div>

            <div className="p-3">
              <div className="mb-1.5 flex items-center justify-between text-[11px] font-semibold text-zinc-500">
                <span>Single Types ({singleTypes.length})</span>
                <button
                  type="button"
                  onClick={() => {
                    setNewKind("singleType");
                    setCreateOpen(true);
                  }}
                  className="text-indigo-700 hover:underline cursor-pointer"
                >
                  + Add
                </button>
              </div>
              <div className="space-y-0.5">
                {singleTypes.map((st) => (
                  <button
                    key={st.uid}
                    type="button"
                    onClick={() => selectSchemaSafely(st.uid)}
                    className={`flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-xs cursor-pointer ${
                      selectedUid === st.uid
                        ? "bg-indigo-50 font-medium text-indigo-900"
                        : "text-zinc-700 hover:bg-zinc-50"
                    }`}
                  >
                    <span className="truncate">{st.displayName}</span>
                    <span className="font-mono text-[10px] text-zinc-400">
                      {st.fields.length}f
                    </span>
                  </button>
                ))}
              </div>
            </div>

            <div className="p-3">
              <div className="mb-1.5 flex items-center justify-between text-[11px] font-semibold text-zinc-500">
                <span>Components ({components.length})</span>
                <button
                  type="button"
                  onClick={() => {
                    setNewKind("component");
                    setCreateOpen(true);
                  }}
                  className="text-indigo-700 hover:underline cursor-pointer"
                >
                  + Add
                </button>
              </div>
              <div className="space-y-0.5">
                {components.map((c) => (
                  <button
                    key={c.uid}
                    type="button"
                    onClick={() => selectSchemaSafely(c.uid)}
                    className={`flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-xs cursor-pointer ${
                      selectedUid === c.uid
                        ? "bg-indigo-50 font-medium text-indigo-900"
                        : "text-zinc-700 hover:bg-zinc-50"
                    }`}
                  >
                    <span className="truncate">{c.displayName}</span>
                    <span className="font-mono text-[10px] text-zinc-400">{c.category}</span>
                  </button>
                ))}
              </div>
            </div>
          </div>
        </Card>

        {/* Right Schema Workspace */}
        <div className="lg:col-span-9">
          {!draft ? (
            <EmptyState
              icon={Blocks}
              title="No content types defined yet"
              description="Create your first collection type, single type, or reusable component to generate SQLite tables and REST/GraphQL schemas."
              action={
                <Button size="sm" onClick={() => setCreateOpen(true)}>
                  <Plus className="h-3.5 w-3.5" />
                  Create first content type
                </Button>
              }
            />
          ) : (
            <Card>
              <CardHeader className="flex-wrap gap-3">
                <div className="flex flex-wrap items-center gap-2">
                  <Layers className="h-4 w-4 text-indigo-700" />
                  <span className="text-sm font-semibold text-zinc-900">{draft.displayName}</span>
                  <Badge tone="mono">{draft.uid}</Badge>
                  <Badge tone="neutral">{draft.kind}</Badge>
                  {dirty && <Badge tone="amber">Unsaved changes</Badge>}
                </div>

                <div className="flex items-center gap-2">
                  <Button size="sm" variant="secondary" onClick={openAddField}>
                    <Plus className="h-3.5 w-3.5" />
                    Add field
                  </Button>
                  <Button
                    size="sm"
                    onClick={() => saveSchemaMut.mutate()}
                    disabled={!dirty || saveSchemaMut.isPending}
                  >
                    <Save className="h-3.5 w-3.5" />
                    {saveSchemaMut.isPending ? "Applying DDL..." : "Save schema"}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setDeleteConfirmOpen(true)}
                    disabled={deleteSchemaMut.isPending}
                  >
                    <Trash2 className="h-3.5 w-3.5 text-zinc-400 hover:text-rose-600" />
                  </Button>
                  <Dialog open={deleteConfirmOpen} onOpenChange={setDeleteConfirmOpen}>
                    <DialogContent
                      title={`Delete "${draft.displayName}"?`}
                      description={
                        draft.kind === "component"
                          ? `Removes component ${draft.uid} if no content type references it.`
                          : `Permanently drops SQLite table and all stored documents for ${draft.uid}.`
                      }
                    >
                      <div className="flex justify-end gap-2 pt-2">
                        <Button
                          type="button"
                          variant="secondary"
                          onClick={() => setDeleteConfirmOpen(false)}
                        >
                          Cancel
                        </Button>
                        <Button
                          type="button"
                          variant="danger"
                          disabled={deleteSchemaMut.isPending}
                          onClick={() => {
                            deleteSchemaMut.mutate(draft.uid);
                            setDeleteConfirmOpen(false);
                          }}
                        >
                          Delete permanently
                        </Button>
                      </div>
                    </DialogContent>
                  </Dialog>
                </div>
              </CardHeader>

              <CardContent>
                <Tabs defaultValue="fields" className="space-y-4">
                  <TabsList>
                    <TabsTrigger value="fields">Fields ({draft.fields.length})</TabsTrigger>
                    <TabsTrigger value="settings">Advanced settings</TabsTrigger>
                    <TabsTrigger value="inspector">SQLite DDL &amp; JSON inspector</TabsTrigger>
                  </TabsList>

                  <TabsContent value="fields">
                    <DataTable<FieldDefinition>
                      rows={draft.fields}
                      getRowKey={(f) => f.name}
                      emptyMessage='No fields yet. Click "Add field" to define attributes, relations, components, or dynamic zones.'
                      columns={[
                        {
                          key: "name",
                          header: "Name",
                          render: (f) => (
                            <span className="font-mono font-medium text-zinc-900">{f.name}</span>
                          ),
                        },
                        {
                          key: "type",
                          header: "Type",
                          render: (f) => <Badge tone="indigo">{f.fieldType}</Badge>,
                        },
                        {
                          key: "details",
                          header: "Configuration & Constraints",
                          render: (f) => (
                            <div className="flex flex-wrap items-center gap-1">
                              {f.required && <Badge tone="amber">required</Badge>}
                              {f.unique && <Badge tone="emerald">unique</Badge>}
                              {f.localized && <Badge tone="neutral">localized</Badge>}
                              {f.private && <Badge tone="rose">private</Badge>}
                              {f.relation && (
                                <Badge tone="mono">
                                  {f.relation.relationType} → {f.relation.targetUid ?? "morph"}
                                </Badge>
                              )}
                              {f.component?.componentUid && (
                                <Badge tone="mono">
                                  {f.component.repeatable ? "repeatable " : ""}
                                  {f.component.componentUid}
                                </Badge>
                              )}
                              {f.component?.components && f.component.components.length > 0 && (
                                <Badge tone="mono">
                                  {f.component.components.length} dz components
                                </Badge>
                              )}
                              {f.validation?.enumValues && f.validation.enumValues.length > 0 && (
                                <Badge tone="mono">
                                  [{f.validation.enumValues.join(", ")}]
                                </Badge>
                              )}
                            </div>
                          ),
                        },
                        {
                          key: "actions",
                          header: "",
                          className: "text-right",
                          render: (f) => {
                            const idx = draft.fields.findIndex((x) => x.name === f.name);
                            return (
                              <div className="inline-flex items-center gap-1">
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  onClick={() => moveField(idx, -1)}
                                  disabled={idx === 0}
                                >
                                  <ArrowUp className="h-3 w-3" />
                                </Button>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  onClick={() => moveField(idx, 1)}
                                  disabled={idx === draft.fields.length - 1}
                                >
                                  <ArrowDown className="h-3 w-3" />
                                </Button>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  onClick={() => openEditField(idx, f)}
                                >
                                  <Pencil className="h-3 w-3" />
                                </Button>
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  onClick={() => removeField(idx)}
                                >
                                  <Trash2 className="h-3 w-3 text-zinc-400 hover:text-rose-600" />
                                </Button>
                              </div>
                            );
                          },
                        },
                      ]}
                    />
                  </TabsContent>

                  <TabsContent value="settings" className="space-y-4">
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                      <div>
                        <label className="mb-1 block text-xs font-medium text-zinc-700">
                          Display name
                        </label>
                        <Input
                          value={draft.displayName}
                          onChange={(e) => {
                            setDraft({ ...draft, displayName: e.target.value });
                            setDirty(true);
                          }}
                        />
                      </div>
                      <div>
                        <label className="mb-1 block text-xs font-medium text-zinc-700">
                          Description
                        </label>
                        <Input
                          value={draft.description ?? ""}
                          onChange={(e) => {
                            setDraft({ ...draft, description: e.target.value });
                            setDirty(true);
                          }}
                        />
                      </div>
                    </div>

                    {draft.kind !== "component" && (
                      <div className="divide-y divide-zinc-200 rounded border border-zinc-200">
                        <label className="flex items-center justify-between px-4 py-3 text-xs">
                          <div>
                            <div className="font-medium text-zinc-900">Draft &amp; publish</div>
                            <div className="text-zinc-500">
                              Isolates draft edits from live published documents per documentId.
                            </div>
                          </div>
                          <Switch
                            checked={draft.draftAndPublish}
                            onCheckedChange={(v) => {
                              setDraft({ ...draft, draftAndPublish: v });
                              setDirty(true);
                            }}
                          />
                        </label>
                        <label className="flex items-center justify-between px-4 py-3 text-xs">
                          <div>
                            <div className="font-medium text-zinc-900">
                              Internationalization (i18n)
                            </div>
                            <div className="text-zinc-500">
                              Allows localized document variants across configured locales.
                            </div>
                          </div>
                          <Switch
                            checked={draft.i18nEnabled}
                            onCheckedChange={(v) => {
                              setDraft({ ...draft, i18nEnabled: v });
                              setDirty(true);
                            }}
                          />
                        </label>
                      </div>
                    )}
                  </TabsContent>

                  <TabsContent value="inspector" className="space-y-4">
                    {inspectionQuery.data?.tableName && (
                      <div className="space-y-2">
                        <div className="flex items-center justify-between">
                          <div className="flex items-center gap-2 text-xs font-semibold text-zinc-900">
                            <Database className="h-3.5 w-3.5 text-indigo-700" />
                            Physical SQLite columns (
                            <code className="font-mono">{inspectionQuery.data.tableName}</code>)
                          </div>
                          {inspectionQuery.data.ftsTableName && (
                            <Badge tone="mono">FTS5: {inspectionQuery.data.ftsTableName}</Badge>
                          )}
                        </div>
                        <div className="flex flex-wrap gap-1.5 rounded border border-zinc-200 bg-zinc-50 p-3">
                          {inspectionQuery.data.columns.map((c) => (
                            <Badge
                              key={c.name}
                              tone={c.primaryKey ? "indigo" : "mono"}
                            >
                              {c.name}: {c.dataType}
                            </Badge>
                          ))}
                        </div>
                      </div>
                    )}

                    {inspectionQuery.data && inspectionQuery.data.migrations.length > 0 && (
                      <div className="space-y-2">
                        <div className="flex items-center gap-2 text-xs font-semibold text-zinc-900">
                          <Code2 className="h-3.5 w-3.5 text-indigo-700" />
                          Applied runtime DDL history ({inspectionQuery.data.migrations.length})
                        </div>
                        <div className="max-h-52 space-y-2 overflow-y-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] text-zinc-100">
                          {inspectionQuery.data.migrations.map((m) => (
                            <div key={m.id} className="border-b border-zinc-800 pb-2 last:border-0">
                              <div className="text-indigo-400">
                                -- #{m.id} [{m.operation}] @ {m.appliedAt}
                              </div>
                              <pre className="mt-1 whitespace-pre-wrap text-zinc-200">
                                {m.ddlSql}
                              </pre>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}

                    <div className="space-y-2">
                      <div className="text-xs font-semibold text-zinc-900">Schema JSON</div>
                      <pre className="max-h-64 overflow-auto rounded border border-zinc-200 bg-zinc-950 p-3 font-mono text-[11px] text-zinc-100">
                        {JSON.stringify(draft, null, 2)}
                      </pre>
                    </div>
                  </TabsContent>
                </Tabs>
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}
