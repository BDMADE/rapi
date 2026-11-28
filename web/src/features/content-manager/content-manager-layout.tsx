import { useQuery } from "@tanstack/react-query";
import { Search, X } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, Outlet, useParams } from "react-router-dom";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";

export function ContentManagerLayout() {
  const { uid } = useParams();
  const [sidebarSearch, setSidebarSearch] = useState("");

  const catalogQuery = useQuery({
    queryKey: ["schemas-catalog"],
    queryFn: api.listSchemas,
  });

  const contentTypes = catalogQuery.data?.contentTypes ?? [];

  const collectionTypes = useMemo(() => {
    const all = contentTypes.filter((s) => s.kind === "collectionType");
    const q = sidebarSearch.trim().toLowerCase();
    if (!q) return all;
    return all.filter(
      (s) =>
        s.displayName.toLowerCase().includes(q) ||
        s.pluralName.toLowerCase().includes(q) ||
        s.uid.toLowerCase().includes(q),
    );
  }, [contentTypes, sidebarSearch]);

  const singleTypes = useMemo(() => {
    const all = contentTypes.filter((s) => s.kind === "singleType");
    const q = sidebarSearch.trim().toLowerCase();
    if (!q) return all;
    return all.filter(
      (s) =>
        s.displayName.toLowerCase().includes(q) ||
        s.singularName.toLowerCase().includes(q) ||
        s.uid.toLowerCase().includes(q),
    );
  }, [contentTypes, sidebarSearch]);

  const totalCollectionCount = contentTypes.filter((s) => s.kind === "collectionType").length;
  const totalSingleCount = contentTypes.filter((s) => s.kind === "singleType").length;

  return (
    <div className="flex min-h-screen bg-[#f6f6f9]">
      {/* Left Strapi Content Manager Sub-Navigation Sidebar */}
      <aside className="sticky top-0 flex h-screen w-[232px] shrink-0 flex-col border-r border-[#eaeaef] bg-white select-none">
        {/* Sidebar Title + Divider */}
        <div className="px-5 pt-5 pb-4">
          <h2 className="text-[18px] leading-6 font-bold tracking-[-0.01em] text-[#32324d]">
            Content Manager
          </h2>
          <div className="mt-4 h-px w-full bg-[#eaeaef]" />
        </div>

        {/* Search Input */}
        <div className="px-4 pb-4">
          <div className="relative flex items-center">
            <Search
              className="pointer-events-none absolute left-3 h-3.5 w-3.5 text-[#8e8ea9]"
              strokeWidth={2}
            />
            <input
              type="text"
              value={sidebarSearch}
              onChange={(e) => setSidebarSearch(e.target.value)}
              placeholder="Search"
              aria-label="Search content types"
              className="h-8 w-full rounded-[4px] border border-[#dcdce4] bg-white pr-7 pl-8 text-[13px] text-[#32324d] placeholder:text-[#8e8ea9] transition-colors focus:border-[#4945ff] focus:ring-1 focus:ring-[#4945ff] focus:outline-none"
            />
            {sidebarSearch && (
              <button
                type="button"
                onClick={() => setSidebarSearch("")}
                aria-label="Clear search"
                className="absolute right-2 rounded p-0.5 text-[#8e8ea9] hover:text-[#32324d] cursor-pointer"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </div>
        </div>

        {/* Scrollable Content Types Lists */}
        <nav className="flex-1 space-y-6 overflow-y-auto px-3 pb-6">
          {/* COLLECTION TYPES */}
          <div>
            <div className="mb-2 flex items-center justify-between px-2">
              <span className="text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase">
                Collection Types
              </span>
              <span className="inline-flex h-5 min-w-[20px] items-center justify-center rounded-[4px] bg-[#eaeaef] px-1.5 text-[11px] font-semibold text-[#666687]">
                {sidebarSearch ? collectionTypes.length : totalCollectionCount}
              </span>
            </div>

            {collectionTypes.length === 0 ? (
              <p className="px-2 py-1.5 text-xs text-[#8e8ea9]">No collection types</p>
            ) : (
              <ul className="space-y-0.5">
                {collectionTypes.map((ct) => {
                  const isActive = ct.uid === uid;
                  return (
                    <li key={ct.uid}>
                      <Link
                        to={`/content-manager/${encodeURIComponent(ct.uid)}`}
                        className={cn(
                          "flex items-center gap-2.5 rounded-[4px] px-3 py-2 text-[13px] transition-colors",
                          isActive
                            ? "bg-[#f0f0ff] font-semibold text-[#4945ff]"
                            : "font-normal text-[#32324d] hover:bg-[#f6f6f9]",
                        )}
                      >
                        <span
                          className={cn(
                            "h-1 w-1 shrink-0 rounded-full",
                            isActive ? "bg-[#4945ff]" : "bg-[#666687]",
                          )}
                        />
                        <span className="truncate">{ct.displayName}</span>
                      </Link>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>

          {/* SINGLE TYPES */}
          <div>
            <div className="mb-2 flex items-center justify-between px-2">
              <span className="text-[11px] font-bold tracking-[0.05em] text-[#666687] uppercase">
                Single Types
              </span>
              <span className="inline-flex h-5 min-w-[20px] items-center justify-center rounded-[4px] bg-[#eaeaef] px-1.5 text-[11px] font-semibold text-[#666687]">
                {sidebarSearch ? singleTypes.length : totalSingleCount}
              </span>
            </div>

            {singleTypes.length === 0 ? (
              <p className="px-2 py-1.5 text-xs text-[#8e8ea9]">No single types</p>
            ) : (
              <ul className="space-y-0.5">
                {singleTypes.map((st) => {
                  const isActive = st.uid === uid;
                  return (
                    <li key={st.uid}>
                      <Link
                        to={`/content-manager/${encodeURIComponent(st.uid)}/single`}
                        className={cn(
                          "flex items-center gap-2.5 rounded-[4px] px-3 py-2 text-[13px] transition-colors",
                          isActive
                            ? "bg-[#f0f0ff] font-semibold text-[#4945ff]"
                            : "font-normal text-[#32324d] hover:bg-[#f6f6f9]",
                        )}
                      >
                        <span
                          className={cn(
                            "h-1 w-1 shrink-0 rounded-full",
                            isActive ? "bg-[#4945ff]" : "bg-[#666687]",
                          )}
                        />
                        <span className="truncate">{st.displayName}</span>
                      </Link>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </nav>
      </aside>

      {/* Main Content Manager Canvas */}
      <div className="min-w-0 flex-1 px-8 py-7 lg:px-10">
        <div className="mx-auto max-w-[1320px]">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
