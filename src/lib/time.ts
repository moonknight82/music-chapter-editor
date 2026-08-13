const TIME_PATTERN = /^(\d+):([0-5]\d):([0-5]\d)(?:\.(\d{1,9}))?$/;

export function parseTimestamp(value: string): number | null {
  const match = value.trim().match(TIME_PATTERN);
  if (!match) return null;
  const fraction = (match[4] ?? "").padEnd(9, "0");
  const nanoseconds = Number(fraction || 0);
  const result =
    Number(match[1]) * 3_600_000_000_000 +
    Number(match[2]) * 60_000_000_000 +
    Number(match[3]) * 1_000_000_000 +
    nanoseconds;
  return Number.isSafeInteger(result) ? result : null;
}

export function formatTimestamp(value: number, compact = false): string {
  const totalSeconds = Math.floor(value / 1_000_000_000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const nanos = Math.max(0, Math.round(value % 1_000_000_000));
  let fraction = nanos.toString().padStart(9, "0").replace(/0+$/, "");
  if (!compact && fraction.length < 3) fraction = fraction.padEnd(3, "0");
  return `${hours.toString().padStart(2, "0")}:${minutes
    .toString()
    .padStart(2, "0")}:${seconds.toString().padStart(2, "0")}${fraction ? `.${fraction}` : ""}`;
}

export function formatDuration(value: number): string {
  if (value < 0 || !Number.isFinite(value)) return "—";
  return formatTimestamp(value, true);
}
