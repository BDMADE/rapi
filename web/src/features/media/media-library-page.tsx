import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  Copy,
  Crop,
  FileText,
  Folder,
  FolderPlus,
  LayoutGrid,
  List as ListIcon,
  Search,
  Trash2,
  UploadCloud,
} from "lucide-react";
import { useRef, useState } from "react";
import {
  Badge,
  Button,
  Card,
  DataTable,
  Dialog,
  DialogContent,
  EmptyState,
  Input,
  Select,
  useToast,
} from "@/components/ui";
import { api, type MediaFileDto } from "@/lib/api";

export function MediaLibraryPage() {
  const { pushToast } = useToast();
  const queryClient = useQueryClient();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [activeFolderId, setActiveFolderId] = useState<number | null>(null);
  const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
  const [search, setSearch] = useState("");
  const [mimeFilter, setMimeFilter] = useState<"all" | "image/" | "application/">("all");
  const [page, setPage] = useState(1);
  const [dragging, setDragging] = useState(false);

  const foldersQuery = useQuery({
    queryKey: ["media-folders"],
    queryFn: api.listMediaFolders,
  });

  const filesQuery = useQuery({
    queryKey: ["media-files", activeFolderId, search, mimeFilter, page],
    queryFn: () =>
      api.listMediaFiles({
        folderId: search.trim() ? undefined : (activeFolderId ?? "root"),
        mimePrefix: mimeFilter === "all" ? undefined : mimeFilter,
        q: search.trim() || undefined,
        page,
        pageSize: 24,
      }),
  });

  const allFolders = foldersQuery.data ?? [];
  const childFolders = allFolders.filter((f) => (f.parentId ?? null) === activeFolderId);
  const currentFolder = allFolders.find((f) => f.id === activeFolderId) ?? null;

  // Folder create dialog
  const [folderOpen, setFolderOpen] = useState(false);
  const [folderName, setFolderName] = useState("");

  const createFolderMut = useMutation({
    mutationFn: () => api.createMediaFolder(folderName, activeFolderId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["media-folders"] });
      setFolderOpen(false);
      setFolderName("");
      pushToast({ title: "Folder created", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Folder error", description: e.message, tone: "error" }),
  });

  const deleteFolderMut = useMutation({
    mutationFn: (id: number) => api.deleteMediaFolder(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["media-folders"] });
      pushToast({ title: "Folder deleted", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Cannot delete folder", description: e.message, tone: "error" }),
  });

  const uploadMut = useMutation({
    mutationFn: (files: File[]) => api.uploadMediaFiles(files, activeFolderId),
    onSuccess: (uploaded) => {
      queryClient.invalidateQueries({ queryKey: ["media-files"] });
      queryClient.invalidateQueries({ queryKey: ["media-folders"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({
        title: `Uploaded ${uploaded.length} asset${uploaded.length === 1 ? "" : "s"}`,
        tone: "success",
      });
    },
    onError: (e: Error) =>
      pushToast({ title: "Upload failed", description: e.message, tone: "error" }),
  });

  // Asset Details / Edit / Crop state
  const [selectedAsset, setSelectedAsset] = useState<MediaFileDto | null>(null);
  const [editName, setEditName] = useState("");
  const [editAlt, setEditAlt] = useState("");
  const [editCaption, setEditCaption] = useState("");
  const [editFolder, setEditFolder] = useState<string>("root");
  const [copiedUrl, setCopiedUrl] = useState(false);

  const [cropOpen, setCropOpen] = useState(false);
  const [cropX, setCropX] = useState(0);
  const [cropY, setCropY] = useState(0);
  const [cropW, setCropW] = useState(200);
  const [cropH, setCropH] = useState(200);
  const [resizeW, setResizeW] = useState("");
  const [resizeH, setResizeH] = useState("");

  const openAssetInspector = (file: MediaFileDto) => {
    setSelectedAsset(file);
    setEditName(file.name);
    setEditAlt(file.alternativeText ?? "");
    setEditCaption(file.caption ?? "");
    setEditFolder(file.folderId ? String(file.folderId) : "root");
  };

  const openCropDialog = (file: MediaFileDto) => {
    setCropX(0);
    setCropY(0);
    setCropW(file.width ?? 200);
    setCropH(file.height ?? 200);
    setResizeW("");
    setResizeH("");
    setCropOpen(true);
  };

  const updateMetaMut = useMutation({
    mutationFn: () =>
      api.updateMediaFile(selectedAsset!.id, {
        name: editName,
        alternativeText: editAlt,
        caption: editCaption,
        folderId: editFolder === "root" ? undefined : Number(editFolder),
        moveToRoot: editFolder === "root",
      }),
    onSuccess: (updated) => {
      setSelectedAsset(updated);
      queryClient.invalidateQueries({ queryKey: ["media-files"] });
      queryClient.invalidateQueries({ queryKey: ["media-folders"] });
      pushToast({ title: "Asset metadata saved", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Update failed", description: e.message, tone: "error" }),
  });

  const cropMut = useMutation({
    mutationFn: () =>
      api.cropMediaFile(selectedAsset!.id, {
        x: cropX,
        y: cropY,
        width: cropW,
        height: cropH,
        resizeWidth: resizeW ? Number(resizeW) : undefined,
        resizeHeight: resizeH ? Number(resizeH) : undefined,
      }),
    onSuccess: (cropped) => {
      setSelectedAsset(cropped);
      setCropOpen(false);
      queryClient.invalidateQueries({ queryKey: ["media-files"] });
      pushToast({ title: "Image cropped & thumbnail regenerated", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Crop failed", description: e.message, tone: "error" }),
  });

  const deleteMut = useMutation({
    mutationFn: ({ id, force }: { id: number; force: boolean }) =>
      api.deleteMediaFile(id, force),
    onSuccess: () => {
      setSelectedAsset(null);
      queryClient.invalidateQueries({ queryKey: ["media-files"] });
      queryClient.invalidateQueries({ queryKey: ["media-folders"] });
      queryClient.invalidateQueries({ queryKey: ["system-overview"] });
      pushToast({ title: "Asset deleted", tone: "success" });
    },
    onError: (e: Error) =>
      pushToast({ title: "Delete blocked", description: e.message, tone: "error" }),
  });

  const files = filesQuery.data?.data ?? [];
  const pagination = filesQuery.data?.meta.pagination;

  return (
    <div className="space-y-5">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold tracking-tight text-zinc-900">Media Library</h1>
          <p className="mt-0.5 text-xs text-zinc-600">
            Local disk storage with SHA-256 deduplication, responsive format worker, and image crop/resize.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button size="sm" variant="secondary" onClick={() => setFolderOpen(true)}>
            <FolderPlus className="h-3.5 w-3.5" />
            New folder
          </Button>
          <Button size="sm" onClick={() => fileInputRef.current?.click()}>
            <UploadCloud className="h-3.5 w-3.5" />
            Upload assets
          </Button>
          <input
            ref={fileInputRef}
            type="file"
            multiple
            className="hidden"
            onChange={(e) => {
              const list = Array.from(e.target.files ?? []);
              if (list.length > 0) uploadMut.mutate(list);
              e.target.value = "";
            }}
          />
        </div>
      </div>

      {/* Create Folder Modal */}
      <Dialog open={folderOpen} onOpenChange={setFolderOpen}>
        <DialogContent title="Create media folder">
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              createFolderMut.mutate();
            }}
          >
            <div>
              <label className="mb-1 block text-xs font-medium text-zinc-700">Folder name</label>
              <Input
                required
                placeholder="Marketing Banners"
                value={folderName}
                onChange={(e) => setFolderName(e.target.value)}
              />
            </div>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="secondary" onClick={() => setFolderOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={createFolderMut.isPending}>
                Create folder
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>

      {/* Drag & Drop Upload Strip */}
      <div
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragging(false);
          const dropped = Array.from(e.dataTransfer.files ?? []);
          if (dropped.length > 0) uploadMut.mutate(dropped);
        }}
        onClick={() => fileInputRef.current?.click()}
        className={`flex cursor-pointer items-center justify-center gap-2.5 rounded-md border border-dashed px-4 py-4 text-xs transition-colors ${
          dragging
            ? "border-indigo-600 bg-indigo-50 text-indigo-900"
            : "border-zinc-300 bg-white text-zinc-600 hover:border-zinc-400"
        }`}
      >
        <UploadCloud className="h-4 w-4 text-indigo-700" />
        <span>
          {uploadMut.isPending
            ? "Uploading and generating thumbnails..."
            : `Drop files here or click to upload into ${currentFolder ? `"${currentFolder.name}"` : "Root (/)"}`}
        </span>
      </div>

      {/* Breadcrumbs + Search/Filter Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-md border border-zinc-200 bg-white p-3">
        <div className="flex items-center gap-1.5 text-xs">
          <button
            type="button"
            onClick={() => setActiveFolderId(null)}
            className={`font-medium cursor-pointer ${
              activeFolderId === null ? "text-indigo-700" : "text-zinc-600 hover:text-zinc-900"
            }`}
          >
            Media Root
          </button>
          {currentFolder && (
            <>
              <span className="text-zinc-400">/</span>
              <span className="font-semibold text-zinc-900">{currentFolder.name}</span>
            </>
          )}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="relative w-56">
            <Search className="pointer-events-none absolute left-2.5 top-2 h-3.5 w-3.5 text-zinc-400" />
            <Input
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setPage(1);
              }}
              placeholder="Search filename, alt, caption..."
              className="pl-8"
            />
          </div>
          <div className="w-36">
            <Select
              value={mimeFilter}
              onValueChange={(v) => {
                setMimeFilter(v as "all" | "image/" | "application/");
                setPage(1);
              }}
              options={[
                { value: "all", label: "All types" },
                { value: "image/", label: "Images" },
                { value: "application/", label: "Documents" },
              ]}
            />
          </div>
          <div className="inline-flex rounded border border-zinc-200 bg-zinc-100 p-0.5">
            <button
              type="button"
              onClick={() => setViewMode("grid")}
              className={`rounded p-1 cursor-pointer ${
                viewMode === "grid" ? "bg-white text-zinc-900" : "text-zinc-500"
              }`}
            >
              <LayoutGrid className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={() => setViewMode("list")}
              className={`rounded p-1 cursor-pointer ${
                viewMode === "list" ? "bg-white text-zinc-900" : "text-zinc-500"
              }`}
            >
              <ListIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>

      {/* Subfolders Row */}
      {!search.trim() && childFolders.length > 0 && (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
          {childFolders.map((folder) => (
            <Card
              key={folder.id}
              className="flex items-center justify-between p-3 hover:border-indigo-300"
            >
              <button
                type="button"
                onClick={() => setActiveFolderId(folder.id)}
                className="flex min-w-0 flex-1 items-center gap-2 text-left cursor-pointer"
              >
                <Folder className="h-4 w-4 shrink-0 text-indigo-700" />
                <div className="truncate">
                  <div className="truncate text-xs font-medium text-zinc-900">{folder.name}</div>
                  <div className="text-[10px] text-zinc-500">{folder.fileCount} files</div>
                </div>
              </button>
              {folder.fileCount === 0 && folder.childCount === 0 && (
                <button
                  type="button"
                  onClick={() => deleteFolderMut.mutate(folder.id)}
                  className="p-1 text-zinc-400 hover:text-rose-600 cursor-pointer"
                  title="Delete empty folder"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              )}
            </Card>
          ))}
        </div>
      )}

      {/* Assets Grid or Table */}
      {files.length === 0 ? (
        <EmptyState
          icon={UploadCloud}
          title="No media assets here"
          description="Upload images, documents, audio, or video files into this folder."
        />
      ) : viewMode === "grid" ? (
        <div className="grid grid-cols-2 gap-3.5 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
          {files.map((file) => {
            const isImg = file.mime.startsWith("image/");
            const thumbUrl = file.formats?.thumbnail?.url ?? file.url;
            return (
              <Card
                key={file.id}
                onClick={() => openAssetInspector(file)}
                className="group cursor-pointer transition-colors hover:border-indigo-400"
              >
                <div className="flex aspect-4/3 items-center justify-center overflow-hidden border-b border-zinc-200 bg-zinc-100">
                  {isImg ? (
                    <img
                      src={thumbUrl}
                      alt={file.alternativeText ?? file.name}
                      className="h-full w-full object-cover"
                    />
                  ) : (
                    <div className="flex flex-col items-center gap-1 text-zinc-500">
                      <FileText className="h-6 w-6" />
                      <span className="font-mono text-[10px] uppercase">{file.ext}</span>
                    </div>
                  )}
                </div>
                <div className="p-2.5">
                  <div className="truncate text-xs font-medium text-zinc-900">{file.name}</div>
                  <div className="mt-1 flex items-center justify-between text-[10px] text-zinc-500">
                    <span>
                      {file.width && file.height ? `${file.width}×${file.height}` : file.ext}
                    </span>
                    <span className="tabular">{file.size} KB</span>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      ) : (
        <DataTable<MediaFileDto>
          rows={files}
          getRowKey={(f) => String(f.id)}
          columns={[
            {
              key: "name",
              header: "Asset",
              render: (f) => (
                <button
                  type="button"
                  onClick={() => openAssetInspector(f)}
                  className="font-medium text-indigo-700 hover:underline cursor-pointer"
                >
                  {f.name}
                </button>
              ),
            },
            {
              key: "mime",
              header: "Type",
              render: (f) => <Badge tone="mono">{f.mime}</Badge>,
            },
            {
              key: "dim",
              header: "Dimensions",
              render: (f) => (
                <span className="font-mono text-[11px] text-zinc-600">
                  {f.width && f.height ? `${f.width}×${f.height}` : "—"}
                </span>
              ),
            },
            {
              key: "size",
              header: "Size",
              render: (f) => <span className="tabular font-mono text-[11px]">{f.size} KB</span>,
            },
            {
              key: "usage",
              header: "Usage",
              render: (f) => (
                <Badge tone={f.usageCount > 0 ? "indigo" : "neutral"}>
                  {f.usageCount} doc{f.usageCount === 1 ? "" : "s"}
                </Badge>
              ),
            },
          ]}
        />
      )}

      {pagination && pagination.pageCount > 1 && (
        <div className="flex items-center justify-between text-xs text-zinc-600">
          <span>
            Page {pagination.page} of {pagination.pageCount}
          </span>
          <div className="flex gap-1.5">
            <Button
              size="sm"
              variant="secondary"
              disabled={page <= 1}
              onClick={() => setPage((p) => p - 1)}
            >
              Previous
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={page >= pagination.pageCount}
              onClick={() => setPage((p) => p + 1)}
            >
              Next
            </Button>
          </div>
        </div>
      )}

      {/* Asset Inspector & Edit Dialog */}
      <Dialog
        open={Boolean(selectedAsset)}
        onOpenChange={(o) => {
          if (!o) setSelectedAsset(null);
        }}
      >
        {selectedAsset && (
          <DialogContent
            title={selectedAsset.name}
            description={`ID #${selectedAsset.id} · ${selectedAsset.mime} · ${selectedAsset.size} KB`}
            className="max-w-xl"
          >
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <div className="flex aspect-4/3 items-center justify-center overflow-hidden rounded border border-zinc-200 bg-zinc-100">
                  {selectedAsset.mime.startsWith("image/") ? (
                    <img
                      src={selectedAsset.url}
                      alt={selectedAsset.alternativeText ?? selectedAsset.name}
                      className="max-h-48 object-contain"
                    />
                  ) : (
                    <FileText className="h-10 w-10 text-zinc-400" />
                  )}
                </div>

                <div className="flex flex-wrap items-center gap-1.5 text-[11px]">
                  {selectedAsset.width && selectedAsset.height && (
                    <Badge tone="mono">
                      {selectedAsset.width}×{selectedAsset.height}px
                    </Badge>
                  )}
                  <Badge tone={selectedAsset.usageCount > 0 ? "indigo" : "neutral"}>
                    Used in {selectedAsset.usageCount} document
                    {selectedAsset.usageCount === 1 ? "" : "s"}
                  </Badge>
                </div>

                <div className="flex flex-wrap gap-1.5 pt-1">
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      navigator.clipboard.writeText(window.location.origin + selectedAsset.url);
                      setCopiedUrl(true);
                      setTimeout(() => setCopiedUrl(false), 1500);
                    }}
                  >
                    {copiedUrl ? (
                      <Check className="h-3.5 w-3.5 text-emerald-600" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                    Copy URL
                  </Button>

                  {selectedAsset.mime.startsWith("image/") && (
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      onClick={() => openCropDialog(selectedAsset)}
                    >
                      <Crop className="h-3.5 w-3.5" />
                      Crop / resize
                    </Button>
                  )}
                </div>
              </div>

              <form
                className="space-y-2.5"
                onSubmit={(e) => {
                  e.preventDefault();
                  updateMetaMut.mutate();
                }}
              >
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">File name</label>
                  <Input value={editName} onChange={(e) => setEditName(e.target.value)} />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">
                    Alternative text (SEO / accessibility)
                  </label>
                  <Input value={editAlt} onChange={(e) => setEditAlt(e.target.value)} />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">Caption</label>
                  <Input value={editCaption} onChange={(e) => setEditCaption(e.target.value)} />
                </div>
                <div>
                  <label className="mb-1 block text-xs font-medium text-zinc-700">Location</label>
                  <Select
                    value={editFolder}
                    onValueChange={setEditFolder}
                    options={[
                      { value: "root", label: "Media Root (/)" },
                      ...allFolders.map((f) => ({ value: String(f.id), label: f.name })),
                    ]}
                  />
                </div>

                <div className="flex items-center justify-between pt-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="danger"
                    disabled={deleteMut.isPending}
                    onClick={() => {
                      if (selectedAsset.usageCount > 0) {
                        if (
                          window.confirm(
                            `This asset is used in ${selectedAsset.usageCount} document(s). Unlink and delete anyway?`,
                          )
                        ) {
                          deleteMut.mutate({ id: selectedAsset.id, force: true });
                        }
                      } else {
                        deleteMut.mutate({ id: selectedAsset.id, force: false });
                      }
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    Delete
                  </Button>
                  <Button type="submit" size="sm" disabled={updateMetaMut.isPending}>
                    Save details
                  </Button>
                </div>
              </form>
            </div>
          </DialogContent>
        )}
      </Dialog>

      {/* Crop & Resize Dialog */}
      <Dialog open={cropOpen} onOpenChange={setCropOpen}>
        <DialogContent
          title="Crop & resize image"
          description="Creates a cropped image using the Rust image engine and regenerates thumbnails."
        >
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              cropMut.mutate();
            }}
          >
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">Origin X (px)</label>
                <Input
                  type="number"
                  min={0}
                  value={cropX}
                  onChange={(e) => setCropX(Number(e.target.value))}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">Origin Y (px)</label>
                <Input
                  type="number"
                  min={0}
                  value={cropY}
                  onChange={(e) => setCropY(Number(e.target.value))}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">
                  Crop width (px)
                </label>
                <Input
                  type="number"
                  min={1}
                  value={cropW}
                  onChange={(e) => setCropW(Number(e.target.value))}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs font-medium text-zinc-700">
                  Crop height (px)
                </label>
                <Input
                  type="number"
                  min={1}
                  value={cropH}
                  onChange={(e) => setCropH(Number(e.target.value))}
                />
              </div>
            </div>

            <div className="grid grid-cols-2 gap-2.5 border-t border-zinc-200 pt-2.5">
              <div>
                <label className="mb-1 block text-[11px] font-medium text-zinc-600">
                  Resize output width (optional)
                </label>
                <Input
                  type="number"
                  min={1}
                  placeholder="Keep cropped width"
                  value={resizeW}
                  onChange={(e) => setResizeW(e.target.value)}
                />
              </div>
              <div>
                <label className="mb-1 block text-[11px] font-medium text-zinc-600">
                  Resize output height (optional)
                </label>
                <Input
                  type="number"
                  min={1}
                  placeholder="Keep cropped height"
                  value={resizeH}
                  onChange={(e) => setResizeH(e.target.value)}
                />
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setCropOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={cropMut.isPending}>
                Apply crop
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
