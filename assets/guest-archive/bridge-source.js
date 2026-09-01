import { unzipSync, zipSync } from "fflate";

const MAX_FILES = 10_000;
const MAX_FILE_BYTES = 8 * 1024 * 1024;
const MAX_WORKSPACE_BYTES = 32 * 1024 * 1024;

function checkedEntries(entries) {
  if (!Array.isArray(entries) || entries.length > MAX_FILES) {
    throw new Error("The archive contains too many entries.");
  }
  let totalBytes = 0;
  for (const entry of entries) {
    const size = entry.content?.length ?? 0;
    if (size > MAX_FILE_BYTES || (totalBytes += size) > MAX_WORKSPACE_BYTES) {
      throw new Error("The archive exceeds the 8 MiB file or 32 MiB workspace limit.");
    }
  }
  return entries;
}

function exportZip(entries) {
  const files = Object.create(null);
  for (const entry of checkedEntries(entries)) {
    files[entry.path] = new Uint8Array(entry.content);
  }
  return zipSync(files, { level: 6 });
}

function importZip(bytes) {
  const files = unzipSync(new Uint8Array(bytes));
  const entries = Object.entries(files).map(([path, content]) => ({
    path,
    directory: path.endsWith("/"),
    content: Array.from(content),
  }));
  return checkedEntries(entries);
}

globalThis.SyntaxisGuestArchive = { exportZip, importZip };
