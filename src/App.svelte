<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "./lib/api";
  import { formatDuration, formatTimestamp, parseTimestamp } from "./lib/time";
  import { parseChapterPaste, type PastePreview } from "./lib/paste";
  import type {
    ChapterDocument,
    ChapterEntry,
    Edition,
    MediumTracklist,
    ReleaseCandidate,
    ReleaseDetails,
    SaveProgress,
    ToolStatus
  } from "./lib/types";

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
  let trackMapping: Array<number | null> = [];
  let mbSearching = false;

  let showSave = false;
  let saveMode: "copy" | "inPlace" = "copy";
  let copyDestination = "";
  let saveProgress: SaveProgress | null = null;

  $: activeEdition = chapterDocument
    ? chapterDocument.editions[chapterDocument.activeEdition] ?? null
    : null;
  $: pastePreview = parseChapterPaste(pasteText);

  onMount(() => {
    void refreshTools();
    const cleanup: Array<() => void> = [];
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
    selectedChapter = insertAt;
    changed();
  }

  function removeChapter() {
    if (!activeEdition || selectedChapter < 0 || chapterDocument?.advanced) return;
    checkpoint();
    activeEdition.chapters.splice(selectedChapter, 1);
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
      trackMapping = [];
    } catch (error) {
      fail(error);
    } finally {
      mbSearching = false;
    }
  }

  function chooseMedium(medium: MediumTracklist) {
    mbMedium = medium;
    trackMapping = activeEdition
      ? activeEdition.chapters.map((_, index) => (index < medium.tracks.length ? index : null))
      : [];
  }

  function setMapping(chapterIndex: number, value: string) {
    trackMapping[chapterIndex] = value === "" ? null : Number(value);
    trackMapping = [...trackMapping];
  }

  function trackUsedElsewhere(trackIndex: number, chapterIndex: number): boolean {
    return trackMapping.some((mapped, index) => mapped === trackIndex && index !== chapterIndex);
  }

  async function applyMapping() {
    if (!chapterDocument || !activeEdition || !mbMedium) return;
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
    <span>{tools?.directory ?? "MKVToolNix not configured"}</span>
  </footer>
</div>

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
          <div class="mapping-heading"><button class="back-button" on:click={() => (mbMedium = null)}>← Mediums</button><div><strong>{mbDetails.artistCredit} — {mbDetails.title}</strong><span>{mbMedium.format ?? "Unknown format"} · {mbMedium.tracks.length} tracks</span></div></div>
          <div class="mapping-grid">
            <div class="mapping-labels"><span>MKV chapter</span><span>MusicBrainz title</span></div>
            {#each activeEdition?.chapters ?? [] as chapter, index}
              <div class="mapping-row"><div><b>{index + 1}</b><span>{chapter.title || "Untitled"}</span></div><select value={trackMapping[index] ?? ""} on:change={(event) => setMapping(index, event.currentTarget.value)}><option value="">Keep current title</option>{#each mbMedium.tracks as track, trackIndex}<option value={trackIndex} disabled={trackUsedElsewhere(trackIndex, index)}>{track.number || track.position}. {track.title}{track.lengthMs ? ` · ${formatDuration(track.lengthMs * 1_000_000)}` : ""}</option>{/each}</select></div>
            {/each}
          </div>
          <div class="mapping-summary"><span>{trackMapping.filter((value) => value !== null).length} mapped</span><span>{mbMedium.tracks.length - new Set(trackMapping.filter((value) => value !== null)).size} unused tracks</span></div>
        {:else if mbDetails}
          <div class="mapping-heading"><button class="back-button" on:click={() => (mbDetails = null)}>← Results</button><div><strong>{mbDetails.artistCredit} — {mbDetails.title}</strong><span>Choose the disc or medium that matches this MKV.</span></div></div>
          <div class="medium-list">{#each mbDetails.media as medium}<button on:click={() => chooseMedium(medium)}><div><b>{medium.position}</b><span><strong>{medium.title || `Medium ${medium.position}`}</strong><small>{medium.format ?? "Unknown format"}</small></span></div><em>{medium.tracks.length} tracks →</em></button>{/each}</div>
        {:else if mbResults.length}
          <div class="release-list">{#each mbResults as release}<button on:click={() => chooseRelease(release)}><div><strong>{release.title}</strong><span>{release.artistCredit}</span><small>{[release.date, release.country, release.status].filter(Boolean).join(" · ")}</small></div><div class="media-chips">{#each release.media as medium}<span class:preferred={mbMedia === "All" || medium.format === mbMedia}>{medium.format ?? "?"} · {medium.trackCount}</span>{/each}</div><b class="score">{release.score}</b></button>{/each}</div>
        {:else}
          <div class="music-empty"><span>♫</span><h3>Search MusicBrainz releases</h3><p>Nothing is applied until you choose a release, select its medium, review every mapping, and press Apply.</p></div>
        {/if}
      </div>
      <div class="modal-actions"><button on:click={() => (showMusicBrainz = false)}>Cancel</button>{#if mbMedium}<button class="primary" on:click={applyMapping}>Apply mapped titles</button>{/if}</div>
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
  <div class="busy-overlay" aria-live="polite"><div class="spinner"></div><strong>{busyLabel}</strong>{#if saveProgress}<progress value={saveProgress.completed} max={Math.max(1, saveProgress.total)}></progress><small>{saveProgress.phase}</small>{#if saveProgress.phase === "copying"}<button on:click={cancelSave}>Cancel</button>{/if}{/if}</div>
{/if}
