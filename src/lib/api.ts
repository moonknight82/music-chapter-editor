import { invoke } from "@tauri-apps/api/core";
import type {
  ChapterDocument,
  FlacExtractionPlan,
  FlacExtractionRequest,
  FlacExtractionResult,
  FlacProbe,
  ImportedChapters,
  ReleaseCandidate,
  ReleaseDetails,
  SaveVerification,
  ToolStatus
} from "./types";

export const api = {
  probeTools: (customPath?: string) =>
    invoke<ToolStatus>("probe_tools", { customPath: customPath ?? null }),
  configureToolsPath: (path: string) =>
    invoke<ToolStatus>("configure_tools_path", { path }),
  openMkv: (path: string) => invoke<ChapterDocument>("open_mkv", { path }),
  importXml: (path: string) => invoke<ImportedChapters>("import_chapter_xml", { path }),
  exportXml: (document: ChapterDocument, path: string) =>
    invoke<void>("export_chapter_xml", { document, path }),
  searchReleases: (request: {
    artist: string;
    release: string;
    media: string;
    releaseId: string | null;
    chapterCount: number;
    refresh: boolean;
  }) => invoke<ReleaseCandidate[]>("search_releases", { request }),
  loadRelease: (id: string, refresh = false) =>
    invoke<ReleaseDetails>("load_release", { id, refresh }),
  applyTrackMapping: (
    document: ChapterDocument,
    editionIndex: number,
    titles: Array<string | null>
  ) => invoke<ChapterDocument>("apply_track_mapping", { document, editionIndex, titles }),
  saveChapters: (
    document: ChapterDocument,
    mode: { kind: "copy"; destination: string } | { kind: "inPlace" }
  ) => invoke<SaveVerification>("save_chapters", { request: { document, mode } }),
  cancelSave: () => invoke<void>("cancel_save"),
  restoreRecovery: (targetPath: string, recoveryPath: string) =>
    invoke<ChapterDocument>("restore_recovery", { targetPath, recoveryPath }),
  probeFlac: (path: string) => invoke<FlacProbe>("probe_flac", { path }),
  previewFlac: (request: FlacExtractionRequest) =>
    invoke<FlacExtractionPlan>("preview_flac_extraction", { request }),
  extractFlac: (request: FlacExtractionRequest) =>
    invoke<FlacExtractionResult>("extract_flac", { request }),
  cancelFlac: () => invoke<void>("cancel_flac")
};
