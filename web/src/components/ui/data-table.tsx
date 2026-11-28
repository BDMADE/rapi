import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface DataColumn<T> {
  key: string;
  header: string;
  className?: string;
  render: (row: T) => ReactNode;
}

interface DataTableProps<T> {
  columns: DataColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  emptyMessage?: string;
}

export function DataTable<T>({
  columns,
  rows,
  getRowKey,
  emptyMessage = "No entries match your criteria.",
}: DataTableProps<T>) {
  return (
    <div className="overflow-x-auto rounded-md border border-zinc-200 bg-white">
      <table className="w-full border-collapse text-left text-xs">
        <thead>
          <tr className="border-b border-zinc-200 bg-zinc-50 text-[11px] font-medium text-zinc-500">
            {columns.map((col) => (
              <th key={col.key} className={cn("px-3.5 py-2.5", col.className)}>
                {col.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-zinc-200 text-zinc-800">
          {rows.length === 0 ? (
            <tr>
              <td colSpan={columns.length} className="px-4 py-8 text-center text-zinc-500">
                {emptyMessage}
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr key={getRowKey(row)} className="hover:bg-zinc-50/80">
                {columns.map((col) => (
                  <td key={col.key} className={cn("px-3.5 py-2.5", col.className)}>
                    {col.render(row)}
                  </td>
                ))}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}
