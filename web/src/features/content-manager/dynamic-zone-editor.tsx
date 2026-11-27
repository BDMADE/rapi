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
import { Badge, Button, Input, Select, Switch } from "@/components/ui";
import type { ContentTypeSchema, FieldDefinition } from "@/lib/api";

interface BlockWithId {
  __dzKey: string;
  __component?: string;
  [key: string]: unknown;
}

function makeKey() {
  return Math.random().toString(36).slice(2, 10);
}

function ensureKeys(items: unknown[]): BlockWithId[] {
  return items.map((raw) => {
    const obj = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
    return {
      ...obj,
      __dzKey: typeof obj.__dzKey === "string" ? obj.__dzKey : makeKey(),
    };
  });
}

function stripKeys(items: BlockWithId[]): Record<string, unknown>[] {
  return items.map(({ __dzKey: _ignored, ...rest }) => rest);
}

function SortableBlockItem({
  item,
  schema,
  onUpdate,
  onRemove,
}: {
  item: BlockWithId;
  schema: ContentTypeSchema | undefined;
  onUpdate: (next: BlockWithId) => void;
  onRemove: () => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition } = useSortable({
    id: item.__dzKey,
  });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="rounded-md border border-zinc-200 bg-zinc-50/70 p-3 space-y-2.5"
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
            {schema?.displayName ?? item.__component ?? "Component"}
          </span>
          {item.__component && <Badge tone="mono">{item.__component}</Badge>}
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
  const rawArr = Array.isArray(value) ? value : [];
  const items = ensureKeys(rawArr);

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = items.findIndex((i) => i.__dzKey === active.id);
    const newIndex = items.findIndex((i) => i.__dzKey === over.id);
    if (oldIndex !== -1 && newIndex !== -1) {
      onChange(stripKeys(arrayMove(items, oldIndex, newIndex)));
    }
  };

  const addBlock = (compUid: string) => {
    const next: BlockWithId[] = [...items, { __dzKey: makeKey(), __component: compUid }];
    onChange(stripKeys(next));
  };

  return (
    <div className="space-y-2.5 rounded-md border border-zinc-200 bg-white p-3">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={items.map((i) => i.__dzKey)} strategy={verticalListSortingStrategy}>
          <div className="space-y-2">
            {items.map((item, idx) => (
              <SortableBlockItem
                key={item.__dzKey}
                item={item}
                schema={item.__component ? componentsByUid.get(item.__component) : undefined}
                onUpdate={(updated) => {
                  const copy = [...items];
                  copy[idx] = updated;
                  onChange(stripKeys(copy));
                }}
                onRemove={() => {
                  onChange(stripKeys(items.filter((_, i) => i !== idx)));
                }}
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

  const rawArr = Array.isArray(value) ? value : [];
  const items = ensureKeys(rawArr);

  return (
    <div className="space-y-2.5 rounded-md border border-zinc-200 bg-white p-3">
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={(ev) => {
          if (!ev.over || ev.active.id === ev.over.id) return;
          const oldIdx = items.findIndex((i) => i.__dzKey === ev.active.id);
          const newIdx = items.findIndex((i) => i.__dzKey === ev.over!.id);
          if (oldIdx !== -1 && newIdx !== -1) {
            onChange(stripKeys(arrayMove(items, oldIdx, newIdx)));
          }
        }}
      >
        <SortableContext items={items.map((i) => i.__dzKey)} strategy={verticalListSortingStrategy}>
          <div className="space-y-2">
            {items.map((item, idx) => (
              <SortableBlockItem
                key={item.__dzKey}
                item={item}
                schema={componentSchema}
                onUpdate={(updated) => {
                  const copy = [...items];
                  copy[idx] = updated;
                  onChange(stripKeys(copy));
                }}
                onRemove={() => onChange(stripKeys(items.filter((_, i) => i !== idx)))}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>

      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => onChange(stripKeys([...items, { __dzKey: makeKey() }]))}
      >
        <Plus className="h-3 w-3" />
        Add {componentSchema.displayName} item
      </Button>
    </div>
  );
}
