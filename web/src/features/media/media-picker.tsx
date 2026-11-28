import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FileText, FolderOpen, Search, UploadCloud, X } from "lucide-react";
import { useRef, useState } from "react";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogTrigger,
  Input,
  useToast,
} from "@/components/ui";
import { api, type MediaFileDto } from "@/lib/api";

function extractMediaIds(value: unknown): number[] {
  if (!value) return [];
  if (typeof value === "number") return [value];
  if (Array.isArray(value)) {
    return value
      .map((item) => {
        if (typeof item === "number") return item;
        if (typeof item === "object" && item !== null && "id" in item) {
          return Number((item as { id: number }).id);
        }
        return 0;
      })
      .filter((n) => n > 0);
  }
  if (typeof value === "object" && value !== null && "id" in value) {
    return [Number((value as { id: number }).id)];
  }
  return [];
}

export function MediaPicker({
  multiple,
  value,
  onChange,
}: {
  multiple: boolean;
  value: unknown;
  onChange: (next: number | number[] | null) => void;
}) {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");

  const selectedIds = extractMediaIds(value);

  const mediaQuery = useQuery({
    queryKey: ["media-files", "picker", search],
    queryFn: () =>
      api.listMediaFiles({
        q: search.trim() || undefined,
        pageSize: 60,
      }),
  });

  const allFiles = mediaQuery.data?.data ?? [];

  const uploadMut = useMutation({
    mutationFn: (files: File[]) => api.uploadMediaFiles(files, null),
    onSuccess: (uploaded) => {
      queryClient.invalidateQueries({ queryKey: ["media-files"] });
      const uploadedIds = uploaded.map((u) => u.id);
      if (multiple) {
        onChange([...selectedIds, ...uploadedIds]);
      } else if (uploadedIds[0]) {
        onChange(uploadedIds[0]);
      }
      setOpen(false);
      pushToast({ title: "Uploaded & attached media", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Upload failed", description: e.message, tone: "error" }),
  });

  const toggleSelect = (file: MediaFileDto) => {
    if (!multiple) {
      onChange(file.id);
      setOpen(false);
      return;
    }
    if (selectedIds.includes(file.id)) {
      onChange(selectedIds.filter((id) => id !== file.id));
    } else {
      onChange([...selectedIds, file.id]);
    }
  };

  const removeId = (id: number) => {
    const remaining = selectedIds.filter((x) => x !== id);
    onChange(multiple ? remaining : (remaining[0] ?? null));
  };

  return (
    <div className="space-y-2.5 rounded-md border border-zinc-200 bg-white p-3">
      {selectedIds.length > 0 && (
        <div className="flex flex-wrap gap-2">
          {selectedIds.map((id) => {
            const file = allFiles.find((f) => f.id === id);
            const thumb = file?.formats?.thumbnail?.url ?? file?.url;
            return (
              <div
                key={id}
                className="relative flex items-center gap-2 rounded border border-zinc-200 bg-zinc-50 p-1.5 pr-2"
              >
                {file && file.mime.startsWith("image/") ? (
                  <img
                    src={thumb}
                    alt={file.name}
                    className="h-9 w-9 rounded object-cover"
                  />
                ) : (
                  <div className="flex h-9 w-9 items-center justify-center rounded bg-zinc-200 text-zinc-600">
                    <FileText className="h-4 w-4" />
                  </div>
                )}
                <div className="max-w-36 truncate text-xs">
                  <div className="truncate font-medium text-zinc-800">
                    {file?.name ?? `Asset #${id}`}
                  </div>
                  <Badge tone="mono">#{id}</Badge>
                </div>
                <button
                  type="button"
                  onClick={() => removeId(id)}
                  className="ml-1 text-zinc-400 hover:text-rose-600 cursor-pointer"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            );
          })}
        </div>
      )}

      <div className="flex items-center gap-2">
        <Dialog open={open} onOpenChange={setOpen}>
          <DialogTrigger asChild>
            <Button type="button" size="sm" variant="secondary">
              <FolderOpen className="h-3.5 w-3.5" />
              Select from Media Library
            </Button>
          </DialogTrigger>
          <DialogContent
            title={multiple ? "Select media assets" : "Select media asset"}
            className="max-w-2xl"
          >
            <div className="space-y-3">
              <div className="flex items-center justify-between gap-2">
                <div className="relative flex-1">
                  <Search className="pointer-events-none absolute left-2.5 top-2 h-3.5 w-3.5 text-zinc-400" />
                  <Input
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    placeholder="Search media by filename, alt text, or caption..."
                    className="h-8 pl-8 text-xs"
                  />
                </div>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => inputRef.current?.click()}
                  disabled={uploadMut.isPending}
                >
                  <UploadCloud className="h-3.5 w-3.5" />
                  {uploadMut.isPending ? "Uploading..." : "Upload new"}
                </Button>
                <input
                  ref={inputRef}
                  type="file"
                  multiple={multiple}
                  className="hidden"
                  onChange={(e) => {
                    const list = Array.from(e.target.files ?? []);
                    if (list.length > 0) uploadMut.mutate(list);
                    e.target.value = "";
                  }}
                />
              </div>

              {allFiles.length === 0 ? (
                <p className="py-8 text-center text-xs text-zinc-500">
                  No media uploaded yet. Click "Upload new" above.
                </p>
              ) : (
                <div className="grid max-h-80 grid-cols-3 gap-2.5 overflow-y-auto sm:grid-cols-4">
                  {allFiles.map((file) => {
                    const active = selectedIds.includes(file.id);
                    const thumb = file.formats?.thumbnail?.url ?? file.url;
                    return (
                      <button
                        key={file.id}
                        type="button"
                        onClick={() => toggleSelect(file)}
                        className={`overflow-hidden rounded border text-left transition-colors cursor-pointer ${
                          active
                            ? "border-indigo-600 ring-2 ring-indigo-600/20"
                            : "border-zinc-200 hover:border-zinc-400"
                        }`}
                      >
                        <div className="flex aspect-4/3 items-center justify-center bg-zinc-100">
                          {file.mime.startsWith("image/") ? (
                            <img
                              src={thumb}
                              alt={file.name}
                              className="h-full w-full object-cover"
                            />
                          ) : (
                            <FileText className="h-6 w-6 text-zinc-400" />
                          )}
                        </div>
                        <div className="truncate p-1.5 text-[11px] font-medium text-zinc-800">
                          {file.name}
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}

              {multiple && (
                <div className="flex justify-end pt-1">
                  <Button type="button" size="sm" onClick={() => setOpen(false)}>
                    Done ({selectedIds.length} selected)
                  </Button>
                </div>
              )}
            </div>
          </DialogContent>
        </Dialog>
      </div>
    </div>
  );
}
