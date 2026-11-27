import { describe, expect, it } from "vitest";
import {
  type StrapiBlockNode,
  strapiBlocksToTiptap,
  tiptapToStrapiBlocks,
} from "@/features/content-manager/blocks-editor";
import { cn, formatUptime } from "./utils";

describe("UI utilities & Strapi v5 Blocks AST serializer", () => {
  it("formats uptime across seconds, minutes, and hours", () => {
    expect(formatUptime(42)).toBe("42s");
    expect(formatUptime(125)).toBe("2m 5s");
    expect(formatUptime(3720)).toBe("1h 2m");
  });

  it("merges Tailwind classes deterministically", () => {
    expect(cn("px-2 py-1", "px-4")).toBe("py-1 px-4");
  });

  it("roundtrips Strapi v5 blocks JSON <-> Tiptap ProseMirror AST", () => {
    const input: StrapiBlockNode[] = [
      {
        type: "heading",
        level: 2,
        children: [{ type: "text", text: "Hello RAPI", bold: true }],
      },
      {
        type: "paragraph",
        children: [{ type: "text", text: "Structured blocks content." }],
      },
    ];
    const tiptapDoc = strapiBlocksToTiptap(input);
    const output = tiptapToStrapiBlocks(tiptapDoc);
    expect(output).toEqual(input);
  });
});
