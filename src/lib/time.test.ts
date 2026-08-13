import { describe, expect, it } from "vitest";
import { formatTimestamp, parseTimestamp } from "./time";

describe("timestamps", () => {
  it("round trips nanosecond timestamps", () => {
    expect(parseTimestamp("01:02:03.123456789")).toBe(3_723_123_456_789);
    expect(formatTimestamp(3_723_123_456_789)).toBe("01:02:03.123456789");
  });

  it("rejects invalid minutes and seconds", () => {
    expect(parseTimestamp("00:61:00")).toBeNull();
    expect(parseTimestamp("00:00:61")).toBeNull();
    expect(parseTimestamp("-00:00:01")).toBeNull();
    expect(parseTimestamp("00:00:01.1234567890")).toBeNull();
  });
});
