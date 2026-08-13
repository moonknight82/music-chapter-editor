import { parseTimestamp } from "./time";

export interface PasteRow {
  title: string;
  startNs: number | null;
}

export interface PastePreview {
  rows: PasteRow[];
  format: "titles" | "title-time" | "ogm";
  errors: string[];
}

export function parseChapterPaste(input: string): PastePreview {
  const lines = input
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  const ogmTimes = new Map<string, string>();
  const ogmNames = new Map<string, string>();
  for (const line of lines) {
    const time = line.match(/^CHAPTER(\d+)=(.+)$/i);
    const name = line.match(/^CHAPTER(\d+)NAME=(.*)$/i);
    if (time) ogmTimes.set(time[1], time[2].trim());
    if (name) ogmNames.set(name[1], name[2].trim());
  }
  if (ogmTimes.size || ogmNames.size) {
    const keys = [...new Set([...ogmTimes.keys(), ...ogmNames.keys()])].sort(
      (a, b) => Number(a) - Number(b)
    );
    const errors: string[] = [];
    const rows = keys.map((key) => {
      const value = ogmTimes.get(key);
      const startNs = value ? parseTimestamp(value) : null;
      if (!value || startNs === null) errors.push(`Chapter ${key} has an invalid or missing time`);
      return { title: ogmNames.get(key) ?? "", startNs };
    });
    return { rows, format: "ogm", errors };
  }

  if (lines.some((line) => line.includes("|"))) {
    const errors: string[] = [];
    const rows = lines.map((line, index) => {
      const split = line.lastIndexOf("|");
      const title = line.slice(0, split).trim();
      const startNs = parseTimestamp(line.slice(split + 1));
      if (startNs === null) errors.push(`Line ${index + 1} has an invalid time`);
      return { title, startNs };
    });
    return { rows, format: "title-time", errors };
  }

  return {
    rows: lines.map((title) => ({ title, startNs: null })),
    format: "titles",
    errors: []
  };
}
