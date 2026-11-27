import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical, Plus, Trash2 } from "lucide-react";
import { useRef } from "react";
import { Badge, Button, Input, Select, Switch } from "@/components/ui";
import type { ContentTypeSchema, FieldDefinition } from "@/lib/api";

function makeKey() {
  return Math.random().toString(36).slice(2, 10);
}

/** Keeps stable React keys per array index across edits without leaking internal keys into saved JSON. */
function useStableBlockKeys(length: number) {
  const keysRef = useRef<string[]>([]);
  while (keysRef.current.length < length) {
    keysRef.current.push(makeKey());
  }
  if (keysRef.current.length > length) {
    keysRef.current.length = length;
  }
  return keysRef;
}

function SortableBlockItem({
  sortId,
  item,
  schema,
  onUpdate,
  onRemove,
}: {
  sortId: string;
  item: Record<string, unknown>;
  schema: ContentTypeSchema | undefined;
  onUpdate: (next: Record<string, unknown>) => void;
  onRemove: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition } = useSortable({
    id: sortId,
  });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="space-y-2.5 rounded-md border border-zinc-200 bg-zinc-50/70 p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <button
            type="button"
            className="cursor-grab text-zinc-400 hover:text-zinc-700"
            {...attributes}
            {...listeners}
          >
            <GripVertical className="h-4 w-4" />
          </button>
          <span className="text-xs font-semibold text-zinc-800">
            {schema?.displayName ??
              (typeof item.__component === "string" ? item.__component : "Component")}
          </span>
          {typeof item.__component === "string" && (
            <Badge tone="mono">{item.__component}</Badge>
          )}
        </div>
        <Button type="button" size="sm" variant="ghost" onClick={onRemove}>
          <Trash2 className="h-3.5 w-3.5 text-zinc-400 hover:text-rose-600" />
        </Button>
      </div>

      {schema && (
        <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
          {schema.fields.map((f) => (
            <ComponentScalarInput
              key={f.name}
              field={f}
              value={item[f.name]}
              onChange={(val) => onUpdate({ ...item, [f.name]: val })}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ComponentScalarInput({
  field,
  value,
  onChange,
}: {
  field: FieldDefinition;
  value: unknown;
  onChange: (val: unknown) => void;
}) {
  if (field.fieldType === "boolean") {
    return (
      <label className="flex items-center justify-between rounded border border-zinc-200 bg-white px-2.5 py-1.5 text-xs">
        <span className="font-medium text-zinc-700">{field.name}</span>
        <Switch checked={Boolean(value)} onCheckedChange={onChange} />
      </label>
    );
  }
  if (field.fieldType === "enumeration") {
    return (
      <div>
        <label className="mb-1 block text-[11px] font-medium text-zinc-600">{field.name}</label>
        <Select
          value={typeof value === "string" ? value : ""}
          onValueChange={onChange}
          options={(field.validation?.enumValues ?? []).map((ev) => ({ value: ev, label: ev }))}
        />
      </div>
    );
  }
  const isNumber =
    field.fieldType === "integer" ||
    field.fieldType === "biginteger" ||
    field.fieldType === "float" ||
    field.fieldType === "decimal";

  return (
    <div>
      <label className="mb-1 block text-[11px] font-medium text-zinc-600">
        {field.name}
        {field.required && <span className="text-rose-600"> *</span>}
      </label>
      <Input
        type={isNumber ? "number" : "text"}
        value={value === undefined || value === null ? "" : String(value)}
        onChange={(e) => {
          const raw = e.target.value;
          onChange(isNumber ? (raw === "" ? null : Number(raw)) : raw);
        }}
      />
    </div>
  );
}

export function DynamicZoneEditor({
  allowedUids,
  componentsByUid,
  value,
  onChange,
}: {
  allowedUids: string[];
  componentsByUid: Map<string, ContentTypeSchema>;
  value: unknown;
  onChange: (blocks: Record<string, unknown>[]) => void;
}) {
  const sensors = useSensors(useSensor(PointerSensor));
  const items: Record<string, unknown>[] = Array.isArray(value)
    ? value.map((v) => (typeof v === "object" && v !== null ? (v as Record<string, unknown>) : {}))
    : [];
  const keysRef = useStableBlockKeys(items.length);

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = keysRef.current.indexOf(String(active.id));
    const newIndex = keysRef.current.indexOf(String(over.id));
    if (oldIndex !== -1 && newIndex !== -1) {
      keysRef.current = arrayMove(keysRef.current, oldIndex, newIndex);
      onChange(arrayMove(items, oldIndex, newIndex));
    }
  };

  const addBlock = (compUid: string) => {
    keysRef.current.push(makeKey());
    onChange([...items, { __component: compUid }]);
  };

  const removeBlock = (idx: number) => {
    keysRef.current.splice(idx, 1);
    onChange(items.filter((_, i) => i !== idx));
  };

  return (
    <div className="space-y-2.5 rounded-md border border-zinc-200 bg-white p-3">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={keysRef.current} strategy={verticalListSortingStrategy}>
          <div className="space-y-2">
            {items.map((item, idx) => (
              <SortableBlockItem
                key={keysRef.current[idx]}
                sortId={keysRef.current[idx]}
                item={item}
                schema={
                  typeof item.__component === "string"
                    ? componentsByUid.get(item.__component)
                    : undefined
                }
                onUpdate={(updated) => {
                  const copy = [...items];
                  copy[idx] = updated;
                  onChange(copy);
                }}
                onRemove={() => removeBlock(idx)}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>

      <div className="flex flex-wrap items-center gap-1.5 pt-1">
        {allowedUids.map((uid) => {
          const comp = componentsByUid.get(uid);
          return (
            <Button
              key={uid}
              type="button"
              size="sm"
              variant="secondary"
              onClick={() => addBlock(uid)}
            >
              <Plus className="h-3 w-3" />
              Add {comp?.displayName ?? uid}
            </Button>
          );
        })}
      </div>
    </div>
  );
}

export function ComponentFieldEditor({
  componentSchema,
  repeatable,
  value,
  onChange,
}: {
  componentSchema: ContentTypeSchema | undefined;
  repeatable: boolean;
  value: unknown;
  onChange: (next: unknown) => void;
}) {
  const sensors = useSensors(useSensor(PointerSensor));
  const items: Record<string, unknown>[] = Array.isArray(value)
    ? value.map((v) => (typeof v === "object" && v !== null ? (v as Record<string, unknown>) : {}))
    : [];
  const keysRef = useStableBlockKeys(items.length);

  if (!componentSchema) {
    return <p className="text-xs text-zinc-500">Component schema unavailable.</p>;
  }

  if (!repeatable) {
    const obj = (typeof value === "object" && value !== null ? value : {}) as Record<
      string,
      unknown
    >;
    return (
      <div className="grid grid-cols-1 gap-2.5 rounded-md border border-zinc-200 bg-zinc-50/70 p-3 sm:grid-cols-2">
        {componentSchema.fields.map((f) => (
          <ComponentScalarInput
            key={f.name}
            field={f}
            value={obj[f.name]}
            onChange={(val) => onChange({ ...obj, [f.name]: val })}
          />
        ))}
      </div>
    );
  }

  return (
    <div className="space-y-2.5 rounded-md border border-zinc-200 bg-white p-3">
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={(ev) => {
          if (!ev.over || ev.active.id === ev.over.id) return;
          const oldIdx = keysRef.current.indexOf(String(ev.active.id));
          const newIdx = keysRef.current.indexOf(String(ev.over.id));
          if (oldIdx !== -1 && newIdx !== -1) {
            keysRef.current = arrayMove(keysRef.current, oldIdx, newIdx);
            onChange(arrayMove(items, oldIdx, newIdx));
          }
        }}
      >
        <SortableContext items={keysRef.current} strategy={verticalListSortingStrategy}>
          <div className="space-y-2">
            {items.map((item, idx) => (
              <SortableBlockItem
                key={keysRef.current[idx]}
                sortId={keysRef.current[idx]}
                item={item}
                schema={componentSchema}
                onUpdate={(updated) => {
                  const copy = [...items];
                  copy[idx] = updated;
                  onChange(copy);
                }}
                onRemove={() => {
                  keysRef.current.splice(idx, 1);
                  onChange(items.filter((_, i) => i !== idx));
                }}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>

      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => {
          keysRef.current.push(makeKey());
          onChange([...items, {}]);
        }}
      >
        <Plus className="h-3 w-3" />
        Add {componentSchema.displayName} item
      </Button>
    </div>
  );
}
