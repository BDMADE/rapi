import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link2, Plus, X } from "lucide-react";
import { useState } from "react";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogTrigger,
  Input,
  Select,
  useToast,
} from "@/components/ui";
import { api, type ContentDocument, type ContentTypeSchema, type RelationKind } from "@/lib/api";

function extractIds(value: unknown): string[] {
  if (!value) return [];
  if (typeof value === "string") return [value];
  if (Array.isArray(value)) {
    return value
      .map((item) => {
        if (typeof item === "string") return item;
        if (typeof item === "object" && item !== null && "documentId" in item) {
          return String((item as { documentId: string }).documentId);
        }
        return "";
      })
      .filter(Boolean);
  }
  if (typeof value === "object" && value !== null) {
    const obj = value as Record<string, unknown>;
    if (Array.isArray(obj.set)) {
      return extractIds(obj.set);
    }
    if (typeof obj.documentId === "string") {
      return [obj.documentId];
    }
  }
  return [];
}

function summarizeDoc(doc: ContentDocument): string {
  for (const key of ["title", "name", "label", "slug", "email"]) {
    if (typeof doc[key] === "string" && (doc[key] as string).trim()) {
      return doc[key] as string;
    }
  }
  return doc.documentId;
}

export function RelationPicker({
  targetUid,
  targetSchema,
  relationKind,
  locale,
  value,
  onChange,
}: {
  targetUid: string;
  targetSchema: ContentTypeSchema | undefined;
  relationKind: RelationKind;
  locale: string;
  value: unknown;
  onChange: (next: { set: { documentId: string }[] }) => void;
}) {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const selectedIds = extractIds(value);
  const isSingle =
    relationKind === "oneToOne" || relationKind === "manyToOne" || relationKind === "morphOne";

  const entriesQuery = useQuery({
    queryKey: ["relation-options", targetUid, locale],
    queryFn: () => api.listEntries(targetUid, { pageSize: 100, locale }),
    enabled: Boolean(targetUid),
  });

  const options = entriesQuery.data?.data ?? [];

  // On-the-fly relation creation dialog
  const [quickOpen, setQuickOpen] = useState(false);
  const [quickTitle, setQuickTitle] = useState("");
  const primaryTextField =
    targetSchema?.fields.find((f) => f.fieldType === "string")?.name ?? "title";

  const quickCreateMut = useMutation({
    mutationFn: () => api.createEntry(targetUid, { [primaryTextField]: quickTitle }, locale),
    onSuccess: (created) => {
      queryClient.invalidateQueries({ queryKey: ["relation-options", targetUid, locale] });
      const nextIds = isSingle
        ? [created.documentId]
        : [...selectedIds.filter((id) => id !== created.documentId), created.documentId];
      onChange({ set: nextIds.map((documentId) => ({ documentId })) });
      setQuickTitle("");
      setQuickOpen(false);
      pushToast({ title: "Related entry created & linked", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Quick create failed", description: e.message, tone: "error" }),
  });

  const addRelation = (docId: string) => {
    if (!docId) return;
    const nextIds = isSingle
      ? [docId]
      : [...selectedIds.filter((x) => x !== docId), docId];
    onChange({ set: nextIds.map((documentId) => ({ documentId })) });
  };

  const removeRelation = (docId: string) => {
    const nextIds = selectedIds.filter((x) => x !== docId);
    onChange({ set: nextIds.map((documentId) => ({ documentId })) });
  };

  return (
    <div className="space-y-2 rounded-md border border-zinc-200 bg-white p-3">
      <div className="flex items-center gap-2">
        <div className="flex-1">
          <Select
            value=""
            onValueChange={addRelation}
            placeholder={`Link ${targetSchema?.displayName ?? targetUid}...`}
            options={options.map((d) => ({
              value: d.documentId,
              label: `${summarizeDoc(d)} (${d.documentId.slice(0, 8)})`,
            }))}
          />
        </div>

        {targetSchema && (
          <Dialog open={quickOpen} onOpenChange={setQuickOpen}>
            <DialogTrigger asChild>
              <Button type="button" size="sm" variant="secondary" title="Create on the fly">
                <Plus className="h-3.5 w-3.5" />
                New
              </Button>
            </DialogTrigger>
            <DialogContent title={`Quick-create ${targetSchema.displayName}`}>
              <div className="space-y-3">
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">
                    {primaryTextField}
                  </label>
                  <Input
                    value={quickTitle}
                    onChange={(e) => setQuickTitle(e.target.value)}
                    placeholder={`Enter ${primaryTextField}...`}
                  />
                </div>
                <div className="flex justify-end gap-2">
                  <Button type="button" variant="secondary" onClick={() => setQuickOpen(false)}>
                    Cancel
                  </Button>
                  <Button
                    type="button"
                    disabled={!quickTitle.trim() || quickCreateMut.isPending}
                    onClick={() => quickCreateMut.mutate()}
                  >
                    Create &amp; connect
                  </Button>
                </div>
              </div>
            </DialogContent>
          </Dialog>
        )}
      </div>

      {selectedIds.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {selectedIds.map((id) => {
            const match = options.find((o) => o.documentId === id);
            return (
              <Badge key={id} tone="indigo" className="gap-1.5 py-1">
                <Link2 className="h-3 w-3" />
                <span>{match ? summarizeDoc(match) : id}</span>
                <button
                  type="button"
                  onClick={() => removeRelation(id)}
                  className="hover:text-rose-700 cursor-pointer"
                >
                  <X className="h-3 w-3" />
                </button>
              </Badge>
            );
          })}
        </div>
      )}
    </div>
  );
}
