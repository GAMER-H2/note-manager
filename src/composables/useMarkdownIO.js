import { save, open } from "@tauri-apps/plugin-dialog";
import { readFile, writeFile } from "@tauri-apps/plugin-fs";
import { sanitizeFileStem } from "../lib/notes.js";

// Single-note markdown import/export. Mirrors useArchive's split: the file is
// picked with the dialog plugin and read/written through the fs plugin (which,
// unlike Rust's std::fs, can open the content:// URIs Android hands back).
// Bytes rather than text so it reuses the fs read-file/write-file permissions
// the app already grants (see capabilities/default.json).
const MARKDOWN_EXTS = ["md", "markdown", "txt"];

export function useMarkdownIO() {
  // Opens a file picker and returns the chosen file's text, or null if the
  // dialog was cancelled.
  const importMarkdown = async () => {
    const path = await open({
      title: "Import markdown",
      multiple: false,
      directory: false,
      filters: [{ name: "Markdown / text", extensions: MARKDOWN_EXTS }],
    });
    if (!path) return null;
    const bytes = await readFile(path);
    return new TextDecoder().decode(bytes);
  };

  // Opens a save dialog (defaulting the name to the note's title) and writes
  // `content` to the chosen file. Returns the path written, or null if cancelled.
  const exportMarkdown = async (title, content) => {
    const stem = sanitizeFileStem(title) || "note";
    const path = await save({
      title: "Export note",
      defaultPath: `${stem}.md`,
      filters: [
        { name: "Markdown", extensions: ["md"] },
        { name: "Plain text", extensions: ["txt"] },
      ],
    });
    if (!path) return null;
    await writeFile(path, new TextEncoder().encode(content ?? ""));
    return path;
  };

  return { importMarkdown, exportMarkdown };
}
