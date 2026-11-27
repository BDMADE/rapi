import { EditorContent, type JSONContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Bold, Code, Heading2, Italic, List, ListOrdered, Quote } from "lucide-react";
import { useEffect, useRef } from "react";

export interface StrapiTextNode {
  type: "text";
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
}

export interface StrapiBlockNode {
  type: "paragraph" | "heading" | "list" | "list-item" | "quote" | "code";
  level?: number;
  format?: "ordered" | "unordered";
  children: Array<StrapiTextNode | StrapiBlockNode>;
}

/** Converts Strapi v5 `blocks` JSON AST into Tiptap ProseMirror JSONContent. */
export function strapiBlocksToTiptap(blocks: unknown): JSONContent {
  if (!Array.isArray(blocks) || blocks.length === 0) {
    return { type: "doc", content: [{ type: "paragraph" }] };
  }

  const content: JSONContent[] = blocks.map((raw) => {
    const b = raw as Partial<StrapiBlockNode>;
    if (b.type === "heading") {
      return {
        type: "heading",
        attrs: { level: b.level ?? 2 },
        content: inlineToTiptap(b.children),
      };
    }
    if (b.type === "quote") {
      return {
        type: "blockquote",
        content: [{ type: "paragraph", content: inlineToTiptap(b.children) }],
      };
    }
    if (b.type === "code") {
      return {
        type: "codeBlock",
        content: inlineToTiptap(b.children),
      };
    }
    if (b.type === "list") {
      const listType = b.format === "ordered" ? "orderedList" : "bulletList";
      const items = (b.children ?? []).map((li) => ({
        type: "listItem",
        content: [
          {
            type: "paragraph",
            content: inlineToTiptap((li as Partial<StrapiBlockNode>).children),
          },
        ],
      }));
      return {
        type: listType,
        content: items.length > 0 ? items : [{ type: "listItem", content: [{ type: "paragraph" }] }],
      };
    }
    return {
      type: "paragraph",
      content: inlineToTiptap(b.children),
    };
  });

  return { type: "doc", content };
}

function inlineToTiptap(children: unknown): JSONContent[] | undefined {
  if (!Array.isArray(children)) return undefined;
  const nodes: JSONContent[] = [];
  for (const ch of children) {
    const t = ch as Partial<StrapiTextNode>;
    if (t.type === "text" && typeof t.text === "string" && t.text.length > 0) {
      const marks: { type: string }[] = [];
      if (t.bold) marks.push({ type: "bold" });
      if (t.italic) marks.push({ type: "italic" });
      if (t.code) marks.push({ type: "code" });
      nodes.push({
        type: "text",
        text: t.text,
        ...(marks.length > 0 ? { marks } : {}),
      });
    }
  }
  return nodes.length > 0 ? nodes : undefined;
}

/** Serializes Tiptap ProseMirror JSONContent back into Strapi v5 `blocks` JSON AST. */
export function tiptapToStrapiBlocks(doc: JSONContent): StrapiBlockNode[] {
  const out: StrapiBlockNode[] = [];
  for (const node of doc.content ?? []) {
    if (node.type === "heading") {
      out.push({
        type: "heading",
        level: Number(node.attrs?.level ?? 2),
        children: tiptapInlineToStrapi(node.content),
      });
    } else if (node.type === "blockquote") {
      const flat: StrapiTextNode[] = [];
      for (const p of node.content ?? []) {
        flat.push(...tiptapInlineToStrapi(p.content));
      }
      out.push({
        type: "quote",
        children: flat.length > 0 ? flat : [{ type: "text", text: "" }],
      });
    } else if (node.type === "codeBlock") {
      out.push({
        type: "code",
        children: tiptapInlineToStrapi(node.content),
      });
    } else if (node.type === "bulletList" || node.type === "orderedList") {
      const items: StrapiBlockNode[] = (node.content ?? []).map((li) => {
        const children: StrapiTextNode[] = [];
        for (const p of li.content ?? []) {
          children.push(...tiptapInlineToStrapi(p.content));
        }
        return {
          type: "list-item",
          children: children.length > 0 ? children : [{ type: "text", text: "" }],
        };
      });
      out.push({
        type: "list",
        format: node.type === "orderedList" ? "ordered" : "unordered",
        children: items,
      });
    } else {
      out.push({
        type: "paragraph",
        children: tiptapInlineToStrapi(node.content),
      });
    }
  }
  return out;
}

function tiptapInlineToStrapi(content?: JSONContent[]): StrapiTextNode[] {
  if (!content || content.length === 0) {
    return [{ type: "text", text: "" }];
  }
  const res: StrapiTextNode[] = [];
  for (const n of content) {
    if (n.type === "text" && typeof n.text === "string") {
      const node: StrapiTextNode = { type: "text", text: n.text };
      for (const m of n.marks ?? []) {
        if (m.type === "bold") node.bold = true;
        if (m.type === "italic") node.italic = true;
        if (m.type === "code") node.code = true;
      }
      res.push(node);
    }
  }
  return res.length > 0 ? res : [{ type: "text", text: "" }];
}

export function BlocksEditor({
  value,
  onChange,
}: {
  value: unknown;
  onChange: (blocks: StrapiBlockNode[]) => void;
}) {
  const internalChange = useRef(false);

  const editor = useEditor({
    extensions: [StarterKit],
    content: strapiBlocksToTiptap(value),
    editorProps: {
      attributes: {
        class:
          "prose prose-sm max-w-none min-h-36 px-3 py-2.5 text-xs text-zinc-900 focus:outline-none",
      },
    },
    onUpdate: ({ editor: ed }) => {
      internalChange.current = true;
      onChange(tiptapToStrapiBlocks(ed.getJSON()));
    },
  });

  useEffect(() => {
    if (!editor) return;
    if (internalChange.current) {
      internalChange.current = false;
      return;
    }
    editor.commands.setContent(strapiBlocksToTiptap(value), { emitUpdate: false });
  }, [value, editor]);

  if (!editor) return null;

  const btn = (active: boolean) =>
    `rounded p-1.5 text-xs cursor-pointer ${
      active ? "bg-indigo-100 text-indigo-800" : "text-zinc-600 hover:bg-zinc-200/70"
    }`;

  return (
    <div className="overflow-hidden rounded-md border border-zinc-300 bg-white focus-within:border-indigo-600">
      <div className="flex flex-wrap items-center gap-0.5 border-b border-zinc-200 bg-zinc-50 px-2 py-1">
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
          className={btn(editor.isActive("heading", { level: 2 }))}
          title="Heading"
        >
          <Heading2 className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleBold().run()}
          className={btn(editor.isActive("bold"))}
          title="Bold"
        >
          <Bold className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleItalic().run()}
          className={btn(editor.isActive("italic"))}
          title="Italic"
        >
          <Italic className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleCode().run()}
          className={btn(editor.isActive("code"))}
          title="Inline code"
        >
          <Code className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleBulletList().run()}
          className={btn(editor.isActive("bulletList"))}
          title="Bullet list"
        >
          <List className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleOrderedList().run()}
          className={btn(editor.isActive("orderedList"))}
          title="Ordered list"
        >
          <ListOrdered className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => editor.chain().focus().toggleBlockquote().run()}
          className={btn(editor.isActive("blockquote"))}
          title="Quote"
        >
          <Quote className="h-3.5 w-3.5" />
        </button>
      </div>
      <EditorContent editor={editor} />
    </div>
  );
}
