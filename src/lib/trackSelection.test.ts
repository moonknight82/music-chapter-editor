import { describe, expect, it } from "vitest";
import {
  initialTrackSelection,
  mappingFromSelection,
  selectionForRange
} from "./trackSelection";

describe("MusicBrainz track selection", () => {
  it("selects and maps every track when counts already match", () => {
    const selection = initialTrackSelection(3, 3);
    expect(selection).toEqual([true, true, true]);
    expect(mappingFromSelection(selection, 3)).toEqual([0, 1, 2]);
  });

  it("requires an explicit subset when a medium contains more tracks than the MKV", () => {
    expect(initialTrackSelection(20, 8)).toEqual(Array(20).fill(false));
  });

  it("maps a non-contiguous subset in MusicBrainz order", () => {
    const selection = [false, true, false, true, true, false];
    expect(mappingFromSelection(selection, 4)).toEqual([1, 3, 4, null]);
  });

  it("selects an inclusive one-based range", () => {
    expect(selectionForRange(8, 3, 6)).toEqual([
      false,
      false,
      true,
      true,
      true,
      true,
      false,
      false
    ]);
  });
});
