export function initialTrackSelection(trackCount: number, chapterCount: number): boolean[] {
  const selectEverything = trackCount > 0 && trackCount === chapterCount;
  return Array.from({ length: trackCount }, () => selectEverything);
}

export function mappingFromSelection(
  selection: readonly boolean[],
  chapterCount: number
): Array<number | null> {
  const selected = selection.flatMap((included, index) => (included ? [index] : []));
  return Array.from({ length: chapterCount }, (_, index) => selected[index] ?? null);
}

export function selectionForRange(trackCount: number, start: number, end: number): boolean[] {
  return Array.from(
    { length: trackCount },
    (_, index) => index + 1 >= start && index + 1 <= end
  );
}
