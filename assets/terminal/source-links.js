const colonLocationPattern = /(?:^|[\s([{\"'])(?:--?>\s*)?((?:\.{1,2}\/)?(?:[A-Za-z0-9_@.+-]+\/)*[A-Za-z0-9_@.+-]+\.[A-Za-z0-9_-]+):(\d+)(?::(\d+))?(?:-(?:(\d+):)?(\d+))?/g;
const parenthesizedLocationPattern = /(?:^|[\s[{\"'])(?:--?>\s*)?((?:\.{1,2}\/)?(?:[A-Za-z0-9_@.+-]+\/)*[A-Za-z0-9_@.+-]+\.[A-Za-z0-9_-]+)\((\d+),(\d+)\)/g;

function collectMatches(text, pattern, parenthesized) {
  pattern.lastIndex = 0;
  return Array.from(text.matchAll(pattern), match => {
    const path = match[1];
    const locationText = match[0].slice(match[0].indexOf(path));
    const line = Number.parseInt(match[2], 10);
    const column = match[3] ? Number.parseInt(match[3], 10) : null;
    const explicitEndLine = !parenthesized && match[4]
      ? Number.parseInt(match[4], 10)
      : null;
    const endColumn = !parenthesized && match[5]
      ? Number.parseInt(match[5], 10)
      : null;
    return {
      start: match.index + match[0].indexOf(path),
      text: locationText,
      path,
      line,
      column,
      endLine: explicitEndLine ?? (endColumn === null ? null : line),
      endColumn,
    };
  });
}

export function parseSourceLocations(text) {
  return [
    ...collectMatches(text, colonLocationPattern, false),
    ...collectMatches(text, parenthesizedLocationPattern, true),
  ].sort((left, right) => left.start - right.start);
}
