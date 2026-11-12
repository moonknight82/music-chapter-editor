<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
  import { getVersion } from "@tauri-apps/api/app";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "./lib/api";
  import { formatDuration, formatTimestamp, parseTimestamp } from "./lib/time";
  import { parseChapterPaste, type PastePreview } from "./lib/paste";
  import {
    initialTrackSelection,
    mappingFromSelection,
    selectionForRange
  } from "./lib/trackSelection";
  import type {
    ChapterDocument,
    ChapterEntry,
    Edition,
    AudioStream,
    FlacExtractionPlan,
    FlacFallbackMetadata,
    FlacProbe,
    FlacProgress,
    MediumTracklist,
    ReleaseCandidate,
    ReleaseDetails,
    SaveProgress,
    ToolStatus
  } from "./lib/types";

  const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000;
  const UPDATE_CHECK_KEY = "music-chapter-editor.last-update-check";

  let tools: ToolStatus | null = null;
  let chapterDocument: ChapterDocument | null = null;
  let activeEdition: Edition | null = null;
  let busy = false;
  let busyLabel = "";
  let errorMessage = "";
  let noticeMessage = "";
  let dirty = false;
  let selectedChapter = -1;
  let history: ChapterDocument[] = [];
  let future: ChapterDocument[] = [];
  let customToolPath = "";

  let appVersion = "0.2.0";
  let showUpdate = false;
  let updateAvailable: Update | null = null;
  let updateChecking = false;
  let updateInstalling = false;
  let updateMessage = "";
  let updateDownloaded = 0;
  let updateTotal = 0;

  let showPaste = false;
  let pasteText = "";
  let pastePreview: PastePreview = parseChapterPaste("");

  let showMusicBrainz = false;
  let mbArtist = "";
  let mbRelease = "";
  let mbMedia = "Blu-ray";
  let mbIdentifier = "";
  let mbResults: ReleaseCandidate[] = [];
  let mbDetails: ReleaseDetails | null = null;
  let mbMedium: MediumTracklist | null = null;
  let selectedTracks: boolean[] = [];
  let trackMapping: Array<number | null> = [];
  let trackRangeStart = 1;
  let trackRangeEnd = 1;
  let selectedTrackCount = 0;
  let mappedTrackCount = 0;
  let mbSearching = false;

  let showSave = false;
  let saveMode: "copy" | "inPlace" = "copy";
  let copyDestination = "";
  let saveProgress: SaveProgress | null = null;

  let showFlac = false;
  let flacProbe: FlacProbe | null = null;
  let flacStreamIndex = -1;
  let selectedAudioStream: AudioStream | null = null;
  let flacOutputParent = "";
  let flacFallback: FlacFallbackMetadata = {
    albumArtist: "",
    album: "",
    releaseYear: "",
    originalYear: "",
    label: "",
    catalogNumber: ""
  };
  let flacPlan: FlacExtractionPlan | null = null;
  let flacProgress: FlacProgress | null = null;
  let flacPreviewing = false;

  $: activeEdition = chapterDocument
    ? chapterDocument.editions[chapterDocument.activeEdition] ?? null
    : null;
  $: pastePreview = parseChapterPaste(pasteText);
  $: selectedTrackCount = selectedTracks.filter(Boolean).length;
  $: mappedTrackCount = trackMapping.filter((value) => value !== null).length;
  $: selectedAudioStream = flacProbe?.streams.find((stream) => stream.index === flacStreamIndex) ?? null;

  onMount(() => {
    void refreshTools();
    const cleanup: Array<() => void> = [];
    void getVersion().then((version) => (appVersion = version)).catch(() => undefined);
    const lastCheck = Number(localStorage.getItem(UPDATE_CHECK_KEY) ?? "0");
    if (!Number.isFinite(lastCheck) || Date.now() - lastCheck >= UPDATE_CHECK_INTERVAL) {
      const updateTimer = window.setTimeout(() => void checkForAppUpdate(false), 5000);
      cleanup.push(() => window.clearTimeout(updateTimer));
    }
    void getCurrentWindow()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          const mkv = event.payload.paths.find((path) => path.toLowerCase().endsWith(".mkv"));
          if (mkv) void loadMkv(mkv);
        }
      })
      .then((unlisten) => cleanup.push(unlisten));
    void listen<SaveProgress>("save-progress", (event) => {
      saveProgress = event.payload;
    }).then((unlisten) => cleanup.push(unlisten));
    void listen<FlacProgress>("flac-progress", (event) => {
      flacProgress = event.payload;
    }).then((unlisten) => cleanup.push(unlisten));
    return () => cleanup.forEach((unlisten) => unlisten());
  });

  function cloneDocument(value: ChapterDocument): ChapterDocument {
    return structuredClone(value);
  }

  function describeError(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return "An unexpected error occurred";
  }

  function clearMessages() {
    errorMessage = "";
    noticeMessage = "";
  }

  async function checkForAppUpdate(showWhenCurrent = true) {
    if (updateChecking || updateInstalling) return;
    if (showWhenCurrent) showUpdate = true;
    updateChecking = true;
    updateMessage = "Checking GitHub for a signed update…";
    updateDownloaded = 0;
    updateTotal = 0;
    if (updateAvailable) {
      await updateAvailable.close();
      updateAvailable = null;
    }
    try {
      const candidate = await checkForUpdate({ timeout: 15_000 });
      localStorage.setItem(UPDATE_CHECK_KEY, String(Date.now()));
      if (candidate) {
        updateAvailable = candidate;
        showUpdate = true;
        updateMessage = `Version ${candidate.version} is ready to install.`;
      } else {
        updateMessage = `Music Chapter Editor ${appVersion} is up to date.`;
      }
    } catch (error) {
      if (showWhenCurrent) {
        showUpdate = true;
        updateMessage = `Could not check for updates: ${describeError(error)}`;
      }
    } finally {
      updateChecking = false;
    }
  }

  async function installAppUpdate() {
    if (!updateAvailable || updateInstalling) return;
    if (dirty && !window.confirm("Installing the update restarts the app and discards unsaved chapter edits. Continue?")) return;
    updateInstalling = true;
    updateMessage = "Downloading and verifying the signed update…";
    updateDownloaded = 0;
    updateTotal = 0;
    try {
      await updateAvailable.downloadAndInstall((event) => {
        if (event.event === "Started") {
          updateTotal = event.data.contentLength ?? 0;
          updateDownloaded = 0;
        } else if (event.event === "Progress") {
          updateDownloaded += event.data.chunkLength;
        } else if (event.event === "Finished") {
          updateMessage = "Update installed. Restarting Music Chapter Editor…";
        }
      });
      await relaunch();
    } catch (error) {
      updateMessage = `The update was not installed: ${describeError(error)}`;
      updateInstalling = false;
    }
  }

  async function closeUpdateDialog() {
    if (updateChecking || updateInstalling) return;
    showUpdate = false;
    if (updateAvailable) await updateAvailable.close();
    updateAvailable = null;
  }

  function fail(error: unknown) {
    errorMessage = describeError(error);
    noticeMessage = "";
  }

  async function withBusy<T>(label: string, operation: () => Promise<T>): Promise<T | null> {
    clearMessages();
    busy = true;
    busyLabel = label;
    try {
      return await operation();
    } catch (error) {
      fail(error);
      return null;
    } finally {
      busy = false;
      busyLabel = "";
    }
  }

  async function refreshTools(path?: string) {
    const result = await withBusy("Checking MKVToolNix…", () => api.probeTools(path));
    if (result) {
      tools = result;
      if (result.directory) customToolPath = result.directory;
    }
  }

  async function chooseToolDirectory() {
    const path = await openDialog({ directory: true, multiple: false, title: "Choose MKVToolNix bin folder" });
    if (typeof path !== "string") return;
    const result = await withBusy("Validating MKVToolNix…", () => api.configureToolsPath(path));
    if (result) {
      tools = result;
      customToolPath = path;
      noticeMessage = "MKVToolNix is configured.";
    }
  }

  async function chooseMkv() {
    if (!tools?.available) return;
    const path = await openDialog({
      multiple: false,
      title: "Open music Blu-ray MKV",
      filters: [{ name: "Matroska video", extensions: ["mkv"] }]
    });
    if (typeof path === "string") await loadMkv(path);
  }

  async function loadMkv(path: string) {
    if (!tools?.available) {
      fail("Install or configure MKVToolNix before opening an MKV.");
      return;
    }
    if (dirty && !window.confirm("Discard the unsaved chapter edits and open another file?")) return;
    const result = await withBusy("Reading MKV chapters…", () => api.openMkv(path));
    if (result) {
      chapterDocument = result;
      mbArtist = result.inferredArtist;
      mbRelease = result.inferredRelease;
      selectedChapter = result.editions[result.activeEdition]?.chapters.length ? 0 : -1;
      history = [];
      future = [];
      dirty = false;
      noticeMessage = result.originalXml
        ? `Loaded ${result.editions.reduce((sum, edition) => sum + edition.chapters.length, 0)} chapters.`
        : "This MKV has no chapters yet.";
    }
  }

  function checkpoint() {
    if (!chapterDocument) return;
    history = [...history.slice(-49), cloneDocument(chapterDocument)];
    future = [];
  }

  function changed() {
    if (!chapterDocument) return;
    chapterDocument = cloneDocument(chapterDocument);
    dirty = true;
  }

  function undo() {
    if (!chapterDocument || history.length === 0) return;
    future = [cloneDocument(chapterDocument), ...future];
    chapterDocument = history[history.length - 1];
    history = history.slice(0, -1);
    dirty = true;
  }

  function redo() {
    if (!chapterDocument || future.length === 0) return;
    history = [...history, cloneDocument(chapterDocument)];
    chapterDocument = future[0];
    future = future.slice(1);
    dirty = true;
  }

  function chooseEdition(index: number) {
    if (!chapterDocument) return;
    chapterDocument.activeEdition = index;
    chapterDocument = cloneDocument(chapterDocument);
    selectedChapter = chapterDocument.editions[index]?.chapters.length ? 0 : -1;
  }

  function updateTitle(index: number, title: string) {
    if (!activeEdition || activeEdition.chapters[index].title === title) return;
    checkpoint();
    activeEdition.chapters[index].title = title;
    changed();
  }

  function updateStart(index: number, text: string, input: HTMLInputElement) {
    if (!activeEdition) return;
    const value = parseTimestamp(text);
    if (value === null) {
      input.value = formatTimestamp(activeEdition.chapters[index].startNs);
      fail("Use HH:MM:SS with up to nine fractional digits.");
      return;
    }
    if (activeEdition.chapters[index].startNs === value) return;
    checkpoint();
    activeEdition.chapters[index].startNs = value;
    changed();
  }

  function suggestedStart(insertAt: number): number | null {
    if (!chapterDocument || !activeEdition) return null;
    if (activeEdition.chapters.length === 0) return 0;
    const previous = activeEdition.chapters[Math.max(0, insertAt - 1)]?.startNs ?? 0;
    const next = activeEdition.chapters[insertAt]?.startNs ?? chapterDocument.durationNs;
    if (next > previous + 1) return previous + Math.floor((next - previous) / 2);
    if (!next && previous + 1_000_000_000 < Number.MAX_SAFE_INTEGER) return previous + 1_000_000_000;
    return null;
  }

  function addChapter() {
    if (!activeEdition || chapterDocument?.advanced) return;
    const insertAt = selectedChapter >= 0 ? selectedChapter + 1 : activeEdition.chapters.length;
    const startNs = suggestedStart(insertAt);
    if (startNs === null) {
      fail("There is no valid timestamp available between these chapters. Edit a nearby start time first.");
      return;
    }
    checkpoint();
    const chapter: ChapterEntry = {
      uid: null,
      startNs,
      endNs: null,
      title: "New chapter",
      displays: [{ title: "New chapter", language: "und", languageIetf: null }],
      sourceIndex: null
    };
    activeEdition.chapters.splice(insertAt, 0, chapter);
    chapterDocument!.musicbrainzSelection = null;
    selectedChapter = insertAt;
    changed();
  }

  function removeChapter() {
    if (!activeEdition || selectedChapter < 0 || chapterDocument?.advanced) return;
    checkpoint();
    activeEdition.chapters.splice(selectedChapter, 1);
    chapterDocument!.musicbrainzSelection = null;
    selectedChapter = Math.min(selectedChapter, activeEdition.chapters.length - 1);
    changed();
  }

  function moveChapter(direction: -1 | 1) {
    if (!activeEdition || selectedChapter < 0 || chapterDocument?.advanced) return;
    const target = selectedChapter + direction;
    if (target < 0 || target >= activeEdition.chapters.length) return;
    checkpoint();
    const timeline = activeEdition.chapters.map(({ startNs, endNs }) => ({ startNs, endNs }));
    [activeEdition.chapters[selectedChapter], activeEdition.chapters[target]] = [
      activeEdition.chapters[target],
      activeEdition.chapters[selectedChapter]
    ];
    activeEdition.chapters.forEach((chapter, index) => Object.assign(chapter, timeline[index]));
    chapterDocument!.musicbrainzSelection = null;
    selectedChapter = target;
    changed();
  }

  function chapterDuration(index: number): number {
    if (!chapterDocument || !activeEdition) return -1;
    const chapter = activeEdition.chapters[index];
    const end = activeEdition.chapters[index + 1]?.startNs ?? chapter.endNs ?? chapterDocument.durationNs;
    return end ? end - chapter.startNs : -1;
  }

  function chapterWarnings(index: number): string[] {
    if (!chapterDocument || !activeEdition) return [];
    const chapter = activeEdition.chapters[index];
    const warnings: string[] = [];
    if (!chapter.title.trim()) warnings.push("Blank title");
    if (index > 0 && chapter.startNs <= activeEdition.chapters[index - 1].startNs)
      warnings.push("Start must be after the previous chapter");
    if (chapterDocument.durationNs && chapter.startNs > chapterDocument.durationNs)
      warnings.push("Starts after the media ends");
    const duration = chapterDuration(index);
    if (duration >= 0 && duration < 1_000_000_000) warnings.push("Shorter than one second");
    if (chapter.endNs !== null && chapter.endNs <= chapter.startNs) warnings.push("End is before start");
    return warnings;
  }

  async function importXml() {
    if (!chapterDocument) return;
    const path = await openDialog({
      multiple: false,
      title: "Import Matroska chapter XML",
      filters: [{ name: "Matroska chapter XML", extensions: ["xml"] }]
    });
    if (typeof path !== "string") return;
    const result = await withBusy("Importing chapter XML…", () => api.importXml(path));
    if (!result || !chapterDocument) return;
    checkpoint();
    chapterDocument.editions = result.editions;
    chapterDocument.activeEdition = 0;
    chapterDocument.advanced = result.advanced;
    chapterDocument.advancedReason = result.advancedReason;
    chapterDocument.originalXml = result.originalXml;
    chapterDocument.musicbrainzSelection = null;
    selectedChapter = result.editions[0]?.chapters.length ? 0 : -1;
    changed();
    noticeMessage = "Imported chapter XML. Save to apply it to the MKV.";
  }

  async function exportXml() {
    if (!chapterDocument) return;
    const defaultPath = chapterDocument.path.replace(/\.mkv$/i, ".chapters.xml");
    const path = await saveDialog({
      title: "Export Matroska chapter XML",
      defaultPath,
      filters: [{ name: "Matroska chapter XML", extensions: ["xml"] }]
    });
    if (!path) return;
    const result = await withBusy("Exporting chapter XML…", () => api.exportXml(chapterDocument!, path));
    if (result !== null) noticeMessage = `Exported ${path}.`;
  }

  function openPasteDialog() {
    if (!chapterDocument || chapterDocument.advanced) return;
    pasteText = "";
    showPaste = true;
  }

  function applyPaste() {
    if (!chapterDocument || !activeEdition || pastePreview.errors.length || !pastePreview.rows.length) return;
    if (pastePreview.format === "titles" && pastePreview.rows.length !== activeEdition.chapters.length) {
      fail(`A titles-only list must contain exactly ${activeEdition.chapters.length} lines.`);
      return;
    }
    checkpoint();
    if (pastePreview.format === "titles") {
      pastePreview.rows.forEach((row, index) => (activeEdition!.chapters[index].title = row.title));
    } else {
      activeEdition.chapters = pastePreview.rows.map((row, index) => {
        const existing = activeEdition!.chapters[index];
        return {
          uid: existing?.uid ?? null,
          startNs: row.startNs!,
          endNs: existing?.endNs ?? null,
          title: row.title,
          displays: existing?.displays ?? [{ title: row.title, language: "und", languageIetf: null }],
          sourceIndex: existing?.sourceIndex ?? null
        };
      });
    }
    chapterDocument.musicbrainzSelection = null;
    selectedChapter = activeEdition.chapters.length ? 0 : -1;
    changed();
    showPaste = false;
    noticeMessage = `Applied ${pastePreview.rows.length} pasted chapters.`;
  }

  function openMusicBrainzDialog() {
    if (!chapterDocument || !activeEdition || chapterDocument.advanced) return;
    mbArtist ||= chapterDocument.inferredArtist;
    mbRelease ||= chapterDocument.inferredRelease;
    mbResults = [];
    mbDetails = null;
    mbMedium = null;
    selectedTracks = [];
    trackMapping = [];
    showMusicBrainz = true;
  }

  async function searchMusicBrainz(refresh = false) {
    if (!activeEdition) return;
    mbSearching = true;
    clearMessages();
    try {
      mbResults = await api.searchReleases({
        artist: mbArtist,
        release: mbRelease,
        media: mbMedia,
        releaseId: mbIdentifier.trim() || null,
        chapterCount: activeEdition.chapters.length,
        refresh
      });
      mbDetails = null;
      mbMedium = null;
      selectedTracks = [];
      trackMapping = [];
      if (!mbResults.length) fail("MusicBrainz found no matching releases.");
    } catch (error) {
      fail(error);
    } finally {
      mbSearching = false;
    }
  }

  async function chooseRelease(release: ReleaseCandidate) {
    mbSearching = true;
    clearMessages();
    try {
      mbDetails = await api.loadRelease(release.id);
      mbMedium = null;
      selectedTracks = [];
      trackMapping = [];
    } catch (error) {
      fail(error);
    } finally {
      mbSearching = false;
    }
  }

  function chooseMedium(medium: MediumTracklist) {
    mbMedium = medium;
    const chapterCount = activeEdition?.chapters.length ?? 0;
    selectedTracks = initialTrackSelection(medium.tracks.length, chapterCount);
    trackMapping = mappingFromSelection(selectedTracks, chapterCount);
    trackRangeStart = 1;
    trackRangeEnd = Math.min(medium.tracks.length, Math.max(1, chapterCount));
  }

  function backToMediums() {
    mbMedium = null;
    selectedTracks = [];
    trackMapping = [];
  }

  function applyTrackSelection(selection: boolean[]) {
    selectedTracks = selection;
    trackMapping = mappingFromSelection(selection, activeEdition?.chapters.length ?? 0);
  }

  function toggleTrackSelection(trackIndex: number, included: boolean) {
    const selection = [...selectedTracks];
    selection[trackIndex] = included;
    applyTrackSelection(selection);
  }

  function selectAllTracks() {
    if (!mbMedium) return;
    applyTrackSelection(Array(mbMedium.tracks.length).fill(true));
  }

  function clearTrackSelection() {
    if (!mbMedium) return;
    applyTrackSelection(Array(mbMedium.tracks.length).fill(false));
  }

  function selectTrackRange() {
    if (!mbMedium) return;
    if (
      !Number.isInteger(trackRangeStart) ||
      !Number.isInteger(trackRangeEnd) ||
      trackRangeStart < 1 ||
      trackRangeEnd < trackRangeStart ||
      trackRangeEnd > mbMedium.tracks.length
    ) {
      fail(`Choose a range between 1 and ${mbMedium.tracks.length}.`);
      return;
    }
    clearMessages();
    applyTrackSelection(
      selectionForRange(mbMedium.tracks.length, trackRangeStart, trackRangeEnd)
    );
  }

  function remapSelectedTracks() {
    trackMapping = mappingFromSelection(selectedTracks, activeEdition?.chapters.length ?? 0);
  }

  function setMapping(chapterIndex: number, value: string) {
    trackMapping[chapterIndex] = value === "" ? null : Number(value);
    trackMapping = [...trackMapping];
  }

  function trackUsedElsewhere(trackIndex: number, chapterIndex: number): boolean {
    return trackMapping.some((mapped, index) => mapped === trackIndex && index !== chapterIndex);
  }

  async function applyMapping() {
    if (!chapterDocument || !activeEdition || !mbMedium || !mbDetails) return;
    const used = trackMapping.filter((value): value is number => value !== null);
    if (new Set(used).size !== used.length) {
      fail("Each MusicBrainz track can be mapped only once.");
      return;
    }
    checkpoint();
    const titles = trackMapping.map((index) => (index === null ? null : mbMedium!.tracks[index]?.title ?? null));
    const result = await withBusy("Applying MusicBrainz titles…", () =>
      api.applyTrackMapping(chapterDocument!, chapterDocument!.activeEdition, titles)
    );
    if (result) {
      result.musicbrainzSelection = {
        releaseId: mbDetails!.id,
        releaseGroupId: mbDetails!.releaseGroupId,
        releaseTitle: mbDetails!.title,
        albumArtist: mbDetails!.artistCredit,
        albumArtistIds: mbDetails!.artistIds,
        releaseDate: mbDetails!.date,
        originalYear: mbDetails!.originalYear,
        label: mbDetails!.label,
        catalogNumber: mbDetails!.catalogNumber,
        mediumPosition: mbMedium.position,
        mediumTitle: mbMedium.title,
        mediumTrackCount: mbMedium.tracks.length,
        totalMedia: mbDetails!.media.length,
        chapterTracks: trackMapping.map((index) => {
          if (index === null) return null;
          const track = mbMedium!.tracks[index];
          return track
            ? {
                trackId: track.id,
                recordingId: track.recordingId,
                position: track.position,
                number: track.number,
                title: track.title,
                artistCredit: track.artistCredit || mbDetails!.artistCredit,
                artistIds: track.artistIds.length ? track.artistIds : mbDetails!.artistIds
              }
            : null;
        })
      };
      chapterDocument = result;
      dirty = true;
      showMusicBrainz = false;
      noticeMessage = "MusicBrainz titles applied to the editor. Review them before saving.";
    }
  }

  function openSaveDialog() {
    if (!chapterDocument || chapterDocument.advanced) return;
    saveMode = "copy";
    copyDestination = chapterDocument.path.replace(/\.mkv$/i, " - chapters.mkv");
    showSave = true;
  }

  async function chooseCopyDestination() {
    if (!chapterDocument) return;
    const path = await saveDialog({
      title: "Save an edited MKV copy",
      defaultPath: copyDestination,
      filters: [{ name: "Matroska video", extensions: ["mkv"] }]
    });
    if (path) copyDestination = path;
  }

  async function performSave() {
    if (!chapterDocument) return;
    if (saveMode === "copy" && !copyDestination) {
      fail("Choose a destination for the MKV copy.");
      return;
    }
    showSave = false;
    saveProgress = { phase: saveMode === "copy" ? "copying" : "writing", completed: 0, total: 1 };
    const mode =
      saveMode === "copy"
        ? ({ kind: "copy", destination: copyDestination } as const)
        : ({ kind: "inPlace" } as const);
    const result = await withBusy("Saving and verifying chapters…", () => api.saveChapters(chapterDocument!, mode));
    saveProgress = null;
    if (result) {
      chapterDocument = result.document;
      history = [];
      future = [];
      dirty = false;
      noticeMessage = `Saved and verified ${result.targetPath}.${
        result.recoveryPath ? ` Recovery: ${result.recoveryPath}.` : ""
      }${result.warnings.length ? ` Warnings: ${result.warnings.join(" ")}` : ""}`;
    }
  }

  async function cancelSave() {
    await api.cancelSave();
  }

  async function openFlacDialog() {
    if (!chapterDocument || !activeEdition || chapterDocument.advanced) return;
    const selection = chapterDocument.musicbrainzSelection;
    flacFallback = {
      albumArtist: selection?.albumArtist || chapterDocument.inferredArtist,
      album: selection?.releaseTitle || chapterDocument.inferredRelease,
      releaseYear: selection?.releaseDate?.slice(0, 4) ?? "",
      originalYear: selection?.originalYear ?? "",
      label: selection?.label ?? "",
      catalogNumber: selection?.catalogNumber ?? ""
    };
    flacOutputParent = "";
    flacPlan = null;
    flacProgress = null;
    const result = await withBusy("Inspecting audio streams…", () => api.probeFlac(chapterDocument!.path));
    if (!result) return;
    flacProbe = result;
    if (!result.available) {
      fail(result.message);
      return;
    }
    if (!result.streams.length) {
      fail("This MKV does not contain an audio stream.");
      return;
    }
    flacStreamIndex = (result.streams.find((stream) => stream.isDefault) ?? result.streams[0]).index;
    showFlac = true;
    await refreshFlacPreview();
  }

  function flacRequest() {
    if (!chapterDocument || !selectedAudioStream) return null;
    return {
      document: chapterDocument,
      stream: selectedAudioStream,
      outputParent: flacOutputParent,
      fallback: flacFallback
    };
  }

  async function refreshFlacPreview() {
    const request = flacRequest();
    if (!request) return;
    flacPreviewing = true;
    clearMessages();
    try {
      flacPlan = await api.previewFlac(request);
    } catch (error) {
      flacPlan = null;
      fail(error);
    } finally {
      flacPreviewing = false;
    }
  }

  async function chooseFlacOutputParent() {
    const path = await openDialog({ directory: true, multiple: false, title: "Choose FLAC output folder" });
    if (typeof path === "string") flacOutputParent = path;
  }

  async function performFlacExtraction() {
    const request = flacRequest();
    if (!request || !flacOutputParent) {
      fail("Choose an output folder before extracting.");
      return;
    }
    if (!flacPlan) await refreshFlacPreview();
    if (!flacPlan) return;
    showFlac = false;
    flacProgress = { phase: "extracting", completed: 0, total: flacPlan.tracks.length, filename: "" };
    const result = await withBusy("Extracting and verifying FLAC tracks…", () => api.extractFlac(request));
    flacProgress = null;
    if (result) {
      noticeMessage = `Extracted and verified ${result.files.length} FLAC files in ${result.outputDirectory}.${
        result.warnings.length ? ` Warnings: ${result.warnings.join(" ")}` : ""
      }`;
    }
  }

  async function cancelFlacExtraction() {
    await api.cancelFlac();
  }

  async function restoreRecovery() {
    if (!chapterDocument) return;
    if (!window.confirm("Restore chapters from a recovery file? The current MKV chapters will be replaced.")) return;
    const path = await openDialog({
      multiple: false,
      title: "Choose chapter recovery file",
      filters: [{ name: "Chapter recovery", extensions: ["xml", "json"] }]
    });
    if (typeof path !== "string") return;
    const result = await withBusy("Restoring and verifying chapters…", () =>
      api.restoreRecovery(chapterDocument!.path, path)
    );
    if (result) {
      chapterDocument = result;
      history = [];
      future = [];
      dirty = false;
      noticeMessage = "Recovery restored and verified.";
    }
  }

  function baseName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }
</script>

<svelte:head><title>Music Chapter Editor</title></svelte:head>

<div class="app-shell">
  <header class="titlebar">
    <div class="brand-mark" aria-hidden="true"><span></span><span></span><b>♪</b></div>
    <div class="brand-copy">
      <h1>Music Chapter Editor</h1>
      <p>{chapterDocument ? baseName(chapterDocument.path) : "Matroska chapters, without the Windows detour"}</p>
    </div>
    <div class:ready={tools?.available} class="tool-pill">
      <i></i>{tools?.available ? `MKVToolNix ${tools.majorVersion}` : "Tools needed"}
    </div>
  </header>

  {#if errorMessage || noticeMessage}
    <div class:error={!!errorMessage} class="message-bar">
      <span>{errorMessage || noticeMessage}</span>
      <button aria-label="Dismiss message" on:click={clearMessages}>×</button>
    </div>
  {/if}

  {#if !tools?.available}
    <main class="setup-view">
      <section class="setup-card">
        <div class="setup-icon">⌁</div>
        <p class="eyebrow">ONE-TIME SETUP</p>
        <h2>Connect MKVToolNix</h2>
        <p>
          Music Chapter Editor delegates Matroska inspection and writing to the trusted MKVToolNix command-line
          tools. Nothing is bundled or downloaded silently.
        </p>
        <div class="command-row"><code>brew install mkvtoolnix</code><button on:click={() => navigator.clipboard.writeText("brew install mkvtoolnix")}>Copy</button></div>
        <div class="setup-actions">
          <button class="primary" on:click={() => refreshTools()}>Check again</button>
          <button on:click={chooseToolDirectory}>Choose bin folder…</button>
          <button class="link-button" on:click={() => openUrl("https://mkvtoolnix.download/downloads.html")}>Official downloads ↗</button>
        </div>
        <details>
          <summary>Locations checked</summary>
          <ul>{#each tools?.searched ?? [] as path}<li><code>{path}</code></li>{/each}</ul>
        </details>
      </section>
    </main>
  {:else}
    <nav class="toolbar" aria-label="Chapter tools">
      <div class="toolbar-group">
        <button class="primary" on:click={chooseMkv}><span>＋</span> Open MKV</button>
        <button disabled={!chapterDocument} on:click={importXml}>Import XML</button>
        <button disabled={!chapterDocument} on:click={exportXml}>Export XML</button>
      </div>
      <div class="toolbar-divider"></div>
      <div class="toolbar-group">
        <button disabled={!chapterDocument || history.length === 0} on:click={undo} title="Undo">↶</button>
        <button disabled={!chapterDocument || future.length === 0} on:click={redo} title="Redo">↷</button>
        <button disabled={!chapterDocument || chapterDocument.advanced} on:click={openPasteDialog}>Bulk paste</button>
        <button disabled={!chapterDocument || chapterDocument.advanced} on:click={openMusicBrainzDialog}>MusicBrainz</button>
      </div>
      <div class="toolbar-spacer"></div>
      <button disabled={!chapterDocument || chapterDocument.advanced || !activeEdition?.chapters.length} on:click={openFlacDialog}>Extract FLAC…</button>
      <button disabled={!chapterDocument} on:click={restoreRecovery}>Restore…</button>
      <button class="save-button" disabled={!chapterDocument || chapterDocument.advanced} on:click={openSaveDialog}>
        Save chapters
      </button>
    </nav>

    {#if !chapterDocument}
      <button class="drop-view" on:click={chooseMkv}>
        <div class="disc-illustration"><i></i><span></span><b>♪</b></div>
        <p class="eyebrow">READY FOR A MUSIC RIP</p>
        <h2>Drop an MKV here</h2>
        <p>or click to choose a music Blu-ray rip</p>
        <small>Video and audio streams are never re-encoded.</small>
      </button>
    {:else}
      <main class="workspace">
        <section class="file-summary">
          <div>
            <p class="eyebrow">OPEN FILE</p>
            <h2>{baseName(chapterDocument.path)}{#if dirty}<span class="dirty-dot" title="Unsaved edits"></span>{/if}</h2>
            <p title={chapterDocument.path}>{chapterDocument.path}</p>
          </div>
          <dl>
            <div><dt>Duration</dt><dd>{formatDuration(chapterDocument.durationNs)}</dd></div>
            <div><dt>Chapters</dt><dd>{activeEdition?.chapters.length ?? 0}</dd></div>
            <div><dt>Editions</dt><dd>{chapterDocument.editions.length}</dd></div>
          </dl>
        </section>

        {#if chapterDocument.advanced}
          <aside class="warning-panel"><strong>Read-only advanced chapters</strong><span>{chapterDocument.advancedReason}</span></aside>
        {/if}

        <section class="editor-panel">
          <div class="editor-heading">
            <div>
              <h3>Chapter timeline</h3>
              <p>Titles and exact start timestamps. Durations are calculated automatically.</p>
            </div>
            <div class="editor-controls">
              {#if chapterDocument.editions.length > 1}
                <label>Edition
                  <select value={chapterDocument.activeEdition} on:change={(event) => chooseEdition(Number(event.currentTarget.value))}>
                    {#each chapterDocument.editions as edition, index}
                      <option value={index}>Edition {index + 1}{edition.default ? " · default" : ""}</option>
                    {/each}
                  </select>
                </label>
              {/if}
              <button disabled={chapterDocument.advanced} on:click={addChapter}>＋ Add</button>
              <button disabled={chapterDocument.advanced || selectedChapter < 0} on:click={removeChapter}>Remove</button>
              <button disabled={chapterDocument.advanced || selectedChapter <= 0} on:click={() => moveChapter(-1)} title="Move chapter up">↑</button>
              <button disabled={chapterDocument.advanced || !activeEdition || selectedChapter >= activeEdition.chapters.length - 1} on:click={() => moveChapter(1)} title="Move chapter down">↓</button>
            </div>
          </div>
          <div class="table-wrap">
            <table>
              <thead><tr><th>#</th><th>Chapter title</th><th>Start</th><th>Duration</th><th aria-label="Status"></th></tr></thead>
              <tbody>
                {#each activeEdition?.chapters ?? [] as chapter, index}
                  {@const warnings = chapterWarnings(index)}
                  <tr class:selected={selectedChapter === index} class:has-warning={warnings.length > 0} on:click={() => (selectedChapter = index)}>
                    <td class="chapter-number">{String(index + 1).padStart(2, "0")}</td>
                    <td><input aria-label={`Chapter ${index + 1} title`} disabled={chapterDocument.advanced} value={chapter.title} on:change={(event) => updateTitle(index, event.currentTarget.value)} /></td>
                    <td><input class="time-input" aria-label={`Chapter ${index + 1} start`} disabled={chapterDocument.advanced} value={formatTimestamp(chapter.startNs)} on:change={(event) => updateStart(index, event.currentTarget.value, event.currentTarget)} /></td>
                    <td class="duration-cell">{formatDuration(chapterDuration(index))}</td>
                    <td class="status-cell">{#if warnings.length}<span title={warnings.join(" · ")}>!</span>{:else}<i title="Valid">✓</i>{/if}</td>
                  </tr>
                {:else}
                  <tr><td colspan="5" class="empty-table">No chapters. Add one, import XML, or paste a timed chapter list.</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>
      </main>
    {/if}
  {/if}

  <footer>
    <span>{chapterDocument ? (dirty ? "Unsaved edits" : "Saved state") : "Local-first · no media upload"}</span>
    <span class="footer-tools">
      <button class:update-ready={!!updateAvailable} disabled={updateChecking || updateInstalling} on:click={() => checkForAppUpdate(true)}>
        {updateChecking ? "Checking…" : updateAvailable ? `Update ${updateAvailable.version}` : `v${appVersion} · Check for updates`}
      </button>
      <span>{tools?.directory ?? "MKVToolNix not configured"}</span>
    </span>
  </footer>
</div>

{#if showUpdate}
  <div class="modal-backdrop" role="presentation">
    <dialog open class="modal update-modal" aria-labelledby="update-title">
      <div class="modal-heading">
        <div><p class="eyebrow">SIGNED APPLICATION UPDATE</p><h2 id="update-title">{updateAvailable ? `Update to ${updateAvailable.version}` : "Application updates"}</h2></div>
        <button class="close" disabled={updateChecking || updateInstalling} on:click={closeUpdateDialog}>×</button>
      </div>
      <div class="update-content">
        <p class:update-error={updateMessage.startsWith("Could not") || updateMessage.startsWith("The update")}>{updateMessage}</p>
        {#if updateAvailable?.body}<div class="release-notes"><strong>What’s new</strong><p>{updateAvailable.body}</p></div>{/if}
        {#if updateInstalling}
          <div class="update-progress">
            <progress value={updateDownloaded} max={Math.max(1, updateTotal)}></progress>
            <span>{updateTotal ? `${Math.min(100, Math.round((updateDownloaded / updateTotal) * 100))}%` : "Working…"}</span>
          </div>
        {/if}
        <small>Updates come from this project’s public GitHub release and are cryptographically verified before installation. The app remains ad-hoc signed and not Apple-notarized.</small>
      </div>
      <div class="modal-actions update-actions">
        <span>Installed version {appVersion}</span>
        <div>
          <button disabled={updateChecking || updateInstalling} on:click={closeUpdateDialog}>Not now</button>
          {#if updateAvailable}
            <button class="primary" disabled={updateInstalling} on:click={installAppUpdate}>{updateInstalling ? "Installing…" : "Install and restart"}</button>
          {:else}
            <button class="primary" disabled={updateChecking} on:click={() => checkForAppUpdate(true)}>{updateChecking ? "Checking…" : "Check again"}</button>
          {/if}
        </div>
      </div>
    </dialog>
  </div>
{/if}

{#if showPaste}
  <div class="modal-backdrop" role="presentation">
    <dialog open class="modal paste-modal" aria-labelledby="paste-title">
      <div class="modal-heading"><div><p class="eyebrow">BULK ENTRY</p><h2 id="paste-title">Paste chapters</h2></div><button class="close" on:click={() => (showPaste = false)}>×</button></div>
      <p>Use one title per line, <code>Title|HH:MM:SS.fff</code>, or MKVToolNix/OGM chapter text.</p>
      <textarea bind:value={pasteText} placeholder="Intro|00:00:00.000&#10;First song|00:01:14.000"></textarea>
      <div class="preview-strip">
        <span><strong>{pastePreview.rows.length}</strong> rows</span>
        <span>Detected: <strong>{pastePreview.format}</strong></span>
        {#if pastePreview.errors.length}<span class="inline-error">{pastePreview.errors[0]}</span>{/if}
      </div>
      <div class="preview-list">
        {#each pastePreview.rows.slice(0, 8) as row, index}<div><b>{index + 1}</b><span>{row.title || "Untitled"}</span><code>{row.startNs === null ? "title only" : formatTimestamp(row.startNs)}</code></div>{/each}
        {#if pastePreview.rows.length > 8}<small>and {pastePreview.rows.length - 8} more…</small>{/if}
      </div>
      <div class="modal-actions"><button on:click={() => (showPaste = false)}>Cancel</button><button class="primary" disabled={!pastePreview.rows.length || pastePreview.errors.length > 0} on:click={applyPaste}>Apply preview</button></div>
    </dialog>
  </div>
{/if}

{#if showMusicBrainz}
  <div class="modal-backdrop" role="presentation">
    <dialog open class="modal music-modal" aria-labelledby="mb-title">
      <div class="modal-heading"><div><p class="eyebrow">ONLINE METADATA</p><h2 id="mb-title">Find chapter titles</h2></div><button class="close" on:click={() => (showMusicBrainz = false)}>×</button></div>
      <div class="search-form">
        <label>Artist<input bind:value={mbArtist} placeholder="The Beatles" /></label>
        <label>Release<input bind:value={mbRelease} placeholder="Abbey Road" /></label>
        <label>Media<select bind:value={mbMedia}><option>Blu-ray</option><option>DVD</option><option>All</option></select></label>
        <button class="primary search-button" disabled={mbSearching} on:click={() => searchMusicBrainz(false)}>{mbSearching ? "Searching…" : "Search"}</button>
      </div>
      <label class="identifier-field">Exact MusicBrainz release URL or MBID <input bind:value={mbIdentifier} placeholder="Optional" /></label>

      <div class="music-content">
        {#if mbMedium && mbDetails}
          <div class="mapping-heading"><button class="back-button" on:click={backToMediums}>← Mediums</button><div><strong>{mbDetails.artistCredit} — {mbDetails.title}</strong><span>{mbMedium.format ?? "Unknown format"} · select the titles belonging to this MKV</span></div></div>
          <div class="track-mapping-layout">
            <section class="track-picker" aria-label="MusicBrainz track selection">
              <div class="track-picker-heading">
                <div><strong>1. Select this file’s titles</strong><span>{selectedTrackCount} of {mbMedium.tracks.length} selected</span></div>
                <div><button on:click={selectAllTracks}>All</button><button on:click={clearTrackSelection}>Clear</button></div>
              </div>
              <div class="range-picker">
                <label>From<input type="number" min="1" max={mbMedium.tracks.length} bind:value={trackRangeStart} /></label>
                <label>Through<input type="number" min="1" max={mbMedium.tracks.length} bind:value={trackRangeEnd} /></label>
                <button on:click={selectTrackRange}>Select range</button>
              </div>
              <div class="track-list">
                {#each mbMedium.tracks as track, trackIndex}
                  <label class:selected={selectedTracks[trackIndex]}>
                    <input type="checkbox" checked={selectedTracks[trackIndex] ?? false} on:change={(event) => toggleTrackSelection(trackIndex, event.currentTarget.checked)} />
                    <b>{track.number || track.position}</b>
                    <span><strong>{track.title}</strong>{#if track.lengthMs}<small>{formatDuration(track.lengthMs * 1_000_000)}</small>{/if}</span>
                  </label>
                {/each}
              </div>
            </section>
            <section class="mapping-panel" aria-label="Chapter title mapping">
              <div class="mapping-panel-heading"><div><strong>2. Review the mapping</strong><span>Selected titles map in order; override any row.</span></div><button disabled={!selectedTrackCount} on:click={remapSelectedTracks}>Map in order</button></div>
              <div class="mapping-grid">
                <div class="mapping-labels"><span>MKV chapter</span><span>Selected MusicBrainz title</span></div>
                {#each activeEdition?.chapters ?? [] as chapter, index}
                  <div class="mapping-row"><div><b>{index + 1}</b><span>{chapter.title || "Untitled"}</span></div><select value={trackMapping[index] ?? ""} on:change={(event) => setMapping(index, event.currentTarget.value)}><option value="">Keep current title</option>{#each mbMedium.tracks as track, trackIndex}{#if selectedTracks[trackIndex]}<option value={trackIndex} disabled={trackUsedElsewhere(trackIndex, index)}>{track.number || track.position}. {track.title}{track.lengthMs ? ` · ${formatDuration(track.lengthMs * 1_000_000)}` : ""}</option>{/if}{/each}</select></div>
                {/each}
              </div>
            </section>
          </div>
          <div class="mapping-summary"><span>{mappedTrackCount} of {activeEdition?.chapters.length ?? 0} chapters mapped</span><span>{selectedTrackCount - new Set(trackMapping.filter((value) => value !== null)).size} selected titles unused</span><span>{mbMedium.tracks.length - selectedTrackCount} other titles excluded from this file</span></div>
        {:else if mbDetails}
          <div class="mapping-heading"><button class="back-button" on:click={() => (mbDetails = null)}>← Results</button><div><strong>{mbDetails.artistCredit} — {mbDetails.title}</strong><span>Choose the disc or medium that matches this MKV.</span></div></div>
          <div class="medium-list">{#each mbDetails.media as medium}<button on:click={() => chooseMedium(medium)}><div><b>{medium.position}</b><span><strong>{medium.title || `Medium ${medium.position}`}</strong><small>{medium.format ?? "Unknown format"}</small></span></div><em>{medium.tracks.length} tracks →</em></button>{/each}</div>
        {:else if mbResults.length}
          <div class="release-list">{#each mbResults as release}<button on:click={() => chooseRelease(release)}><div><strong>{release.title}</strong><span>{release.artistCredit}</span><small>{[release.date, release.country, release.status].filter(Boolean).join(" · ")}</small></div><div class="media-chips">{#each release.media as medium}<span class:preferred={mbMedia === "All" || medium.format === mbMedia}>{medium.format ?? "?"} · {medium.trackCount}</span>{/each}</div><b class="score">{release.score}</b></button>{/each}</div>
        {:else}
          <div class="music-empty"><span>♫</span><h3>Search MusicBrainz releases</h3><p>Nothing is applied until you choose a release, select its medium, review every mapping, and press Apply.</p></div>
        {/if}
      </div>
      <div class="modal-actions"><button on:click={() => (showMusicBrainz = false)}>Cancel</button>{#if mbMedium}<button class="primary" disabled={mappedTrackCount === 0} on:click={applyMapping}>Apply mapped titles</button>{/if}</div>
    </dialog>
  </div>
{/if}

{#if showFlac && chapterDocument && flacProbe}
  <div class="modal-backdrop" role="presentation">
    <dialog open class="modal flac-modal" aria-labelledby="flac-title">
      <div class="modal-heading">
        <div><p class="eyebrow">LOSSLESS AUDIO EXPORT</p><h2 id="flac-title">Extract chapters as FLAC</h2></div>
        <button class="close" on:click={() => (showFlac = false)}>×</button>
      </div>
      <div class="flac-content">
        <section>
          <div class="section-heading"><strong>1. Audio stream</strong><span>Only the selected stream is decoded. The MKV is never changed.</span></div>
          <div class="audio-streams">
            {#each flacProbe.streams as stream}
              <label class:selected={flacStreamIndex === stream.index}>
                <input type="radio" bind:group={flacStreamIndex} value={stream.index} on:change={refreshFlacPreview} />
                <span><strong>{stream.title || stream.codecLongName || stream.codec}</strong><small>{[stream.language, stream.channelLayout || `${stream.channels} channels`, stream.sampleRate ? `${stream.sampleRate / 1000} kHz` : null, stream.bitDepth ? `${stream.bitDepth}-bit` : null, stream.isDefault ? "default" : null].filter(Boolean).join(" · ")}</small></span>
              </label>
            {/each}
          </div>
        </section>

        <section>
          <div class="section-heading"><strong>2. Album metadata</strong><span>{chapterDocument.musicbrainzSelection ? "From the selected MusicBrainz release and track mapping" : "Editable fallback metadata"}</span></div>
          {#if chapterDocument.musicbrainzSelection}
            <aside class="metadata-source">MusicBrainz metadata is authoritative for this export. “Keep current” chapter rows still use their chapter title.</aside>
          {/if}
          <div class="metadata-grid">
            <label>Album artist<input bind:value={flacFallback.albumArtist} disabled={!!chapterDocument.musicbrainzSelection} /></label>
            <label>Album<input bind:value={flacFallback.album} disabled={!!chapterDocument.musicbrainzSelection} /></label>
            <label>Original year<input bind:value={flacFallback.originalYear} disabled={!!chapterDocument.musicbrainzSelection} /></label>
            <label>Release year<input bind:value={flacFallback.releaseYear} disabled={!!chapterDocument.musicbrainzSelection} /></label>
            <label>Label<input bind:value={flacFallback.label} disabled={!!chapterDocument.musicbrainzSelection} /></label>
            <label>Catalog number<input bind:value={flacFallback.catalogNumber} disabled={!!chapterDocument.musicbrainzSelection} /></label>
          </div>
          {#if !chapterDocument.musicbrainzSelection}<button class="refresh-preview" disabled={flacPreviewing} on:click={refreshFlacPreview}>{flacPreviewing ? "Updating…" : "Update naming preview"}</button>{/if}
        </section>

        <section>
          <div class="section-heading"><strong>3. Destination and preview</strong><span>Picard Preset 3, plus the selected stream’s channel count</span></div>
          <label class="flac-destination">Output parent folder<div><input readonly value={flacOutputParent} placeholder="Choose where the Album Artist folder will be created" /><button on:click={chooseFlacOutputParent}>Choose…</button></div></label>
          {#if flacPlan}
            <div class="naming-preview">
              <code>{flacPlan.albumDirectory}/</code>
              <div>{#each flacPlan.tracks as track}<span><b>{track.chapterIndex + 1}</b><code>{track.relativePath.split("/").slice(-1)[0]}</code><small>{formatDuration(track.endNs - track.startNs)}</small></span>{/each}</div>
            </div>
            {#if flacPlan.warnings.length}<aside class="flac-warnings">{flacPlan.warnings.join(" ")}</aside>{/if}
          {/if}
        </section>
      </div>
      <div class="modal-actions"><span class="flac-tool">{flacProbe.version ?? flacProbe.message}</span><button on:click={() => (showFlac = false)}>Cancel</button><button class="primary" disabled={!flacOutputParent || !flacPlan || flacPreviewing} on:click={performFlacExtraction}>Extract {flacPlan?.tracks.length ?? 0} tracks</button></div>
    </dialog>
  </div>
{/if}

{#if showSave && chapterDocument}
  <div class="modal-backdrop" role="presentation">
    <dialog open class="modal save-modal" aria-labelledby="save-title">
      <div class="modal-heading"><div><p class="eyebrow">SAFE WRITE</p><h2 id="save-title">Save chapters</h2></div><button class="close" on:click={() => (showSave = false)}>×</button></div>
      <div class="save-choices">
        <label class:selected={saveMode === "copy"}><input type="radio" bind:group={saveMode} value="copy" /><span><strong>Save an edited copy <em>Recommended</em></strong><small>Your source remains byte-for-byte untouched. APFS cloning is used when available.</small></span></label>
        <label class:selected={saveMode === "inPlace"}><input type="radio" bind:group={saveMode} value="inPlace" /><span><strong>Edit the open MKV in place</strong><small>A timestamped recovery XML is written beside the MKV before any change.</small></span></label>
      </div>
      {#if saveMode === "copy"}<label class="destination-field">Destination<div><input readonly value={copyDestination} /><button on:click={chooseCopyDestination}>Choose…</button></div></label>{/if}
      <aside class="save-note">MKVToolNix changes only the chapter metadata. The app re-extracts the result and verifies it before reporting success.</aside>
      <div class="modal-actions"><button on:click={() => (showSave = false)}>Cancel</button><button class="primary" on:click={performSave}>{saveMode === "copy" ? "Save and verify copy" : "Back up, save, and verify"}</button></div>
    </dialog>
  </div>
{/if}

{#if busy}
  <div class="busy-overlay" aria-live="polite"><div class="spinner"></div><strong>{busyLabel}</strong>{#if saveProgress}<progress value={saveProgress.completed} max={Math.max(1, saveProgress.total)}></progress><small>{saveProgress.phase}</small>{#if saveProgress.phase === "copying"}<button on:click={cancelSave}>Cancel</button>{/if}{:else if flacProgress}<progress value={flacProgress.completed} max={Math.max(1, flacProgress.total)}></progress><small>{flacProgress.phase} {flacProgress.filename}</small><button on:click={cancelFlacExtraction}>Cancel extraction</button>{/if}</div>
{/if}
