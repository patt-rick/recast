/** Reorderable list on pointer events (dnd-kit), not HTML5 drag-and-drop: Tauri's
 *  dragDropEnabled must stay on for file drops, and that disables HTML5 DnD (§4.5). */
import { DndContext, KeyboardSensor, PointerSensor, closestCenter, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { SortableContext, arrayMove, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { formatBytes } from "../shared/format";
import type { ToolItem } from "./toolList";

function Row({ item, onRemove }: { item: ToolItem; onRemove: () => void }) {
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({ id: item.id });
  return (
    <li ref={setNodeRef} className={isDragging ? "row sortable dragging" : "row sortable"} style={{ transform: CSS.Transform.toString(transform), transition }}>
      <button ref={setActivatorNodeRef} className="handle ghost" aria-label={`Move ${item.name}`} {...attributes} {...listeners}>
        ⋮⋮
      </button>
      <div>
        <strong>{item.name}</strong>
        <div className="muted">{[formatBytes(item.size), item.pages ? `${item.pages} pages` : null].filter(Boolean).join(" · ")}</div>
      </div>
      <button className="ghost" aria-label={`Remove ${item.name}`} onClick={onRemove}>
        ✕
      </button>
    </li>
  );
}

export function SortableList({ items, onChange }: { items: ToolItem[]; onChange: (next: ToolItem[]) => void }) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return;
    const from = items.findIndex((i) => i.id === active.id);
    const to = items.findIndex((i) => i.id === over.id);
    onChange(arrayMove(items, from, to));
  };
  return (
    <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
      <SortableContext items={items.map((i) => i.id)} strategy={verticalListSortingStrategy}>
        <ol className="rows tool-list">
          {items.map((item) => (
            <Row key={item.id} item={item} onRemove={() => onChange(items.filter((i) => i.id !== item.id))} />
          ))}
        </ol>
      </SortableContext>
    </DndContext>
  );
}
