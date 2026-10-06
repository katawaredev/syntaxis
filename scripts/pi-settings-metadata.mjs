// Pi documents settings in Markdown tables; escaped pipes belong to a cell.
export function parseSettingsMarkdown(documentation) {
  const documented = new Map();
  for (const line of documentation.split("\n")) {
    if (!line.startsWith("|")) continue;
    const cells = line.split(/(?<!\\)\|/).slice(1, -1);
    if (cells.length !== 4) continue;
    const [rawPath, rawType, rawDefault, rawDescription] = cells.map((cell) =>
      cell.trim().replaceAll("\\|", "|"),
    );
    const path = rawPath.match(/^`([^`]+)`$/)?.[1];
    if (!path) continue;
    const type = rawType.replaceAll("`", "");
    let defaultValue = rawDefault === "-" ? "" : rawDefault.replace(/^`(.*)`$/, "$1");
    defaultValue = defaultValue.replace(/^"(.*)"$/, "$1");
    if (type === "number" && !/^\d+$/.test(defaultValue)) defaultValue = "";
    documented.set(path, {
      type,
      defaultValue,
      description: rawDescription.replaceAll("`", ""),
    });
  }
  return documented;
}
