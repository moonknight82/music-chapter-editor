import { describe, expect, it } from "vitest";
import { parseChapterPaste } from "./paste";

describe("chapter paste", () => {
  it("parses OGM chapters", () => {
    const result = parseChapterPaste(
      "CHAPTER01=00:00:00.000\nCHAPTER01NAME=Intro\nCHAPTER02=00:01:14.000\nCHAPTER02NAME=Song"
    );
    expect(result.format).toBe("ogm");
    expect(result.rows).toHaveLength(2);
    expect(result.rows[1].title).toBe("Song");
    expect(result.rows[1].startNs).toBe(74_000_000_000);
  });

  it("parses title and time lines", () => {
    const result = parseChapterPaste("Intro|00:00:00.000\nSong|00:01:00.000");
    expect(result.format).toBe("title-time");
    expect(result.errors).toEqual([]);
  });

  it("keeps Unicode titles in a titles-only list", () => {
    const result = parseChapterPaste("Prólogo\n第二部\nФинал");
    expect(result.format).toBe("titles");
    expect(result.rows.map((row) => row.title)).toEqual(["Prólogo", "第二部", "Финал"]);
    expect(result.rows.every((row) => row.startNs === null)).toBe(true);
  });

  it("previews malformed timed input without applying guessed times", () => {
    const ogm = parseChapterPaste("CHAPTER01NAME=Intro\nCHAPTER02=not-a-time");
    expect(ogm.errors).toHaveLength(2);
    expect(ogm.rows.every((row) => row.startNs === null)).toBe(true);

    const titled = parseChapterPaste("Song|00:99:00");
    expect(titled.errors).toEqual(["Line 1 has an invalid time"]);
  });
});
