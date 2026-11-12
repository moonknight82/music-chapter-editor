export interface FileIdentity {
  device: string;
  inode: string;
  size: number;
  modifiedNs: string;
}

export interface ChapterDisplay {
  title: string;
  language: string;
  languageIetf: string | null;
}

export interface ChapterEntry {
  uid: string | null;
  startNs: number;
  endNs: number | null;
  title: string;
  displays: ChapterDisplay[];
  sourceIndex: number | null;
}

export interface Edition {
  uid: string | null;
  hidden: boolean;
  default: boolean;
  ordered: boolean;
  chapters: ChapterEntry[];
  sourceIndex: number | null;
}

export interface ChapterDocument {
  path: string;
  durationNs: number;
  fileIdentity: FileIdentity;
  editions: Edition[];
  activeEdition: number;
  advanced: boolean;
  advancedReason: string | null;
  originalXml: string | null;
  inferredArtist: string;
  inferredRelease: string;
  musicbrainzSelection: MusicBrainzSelection | null;
}

export interface MusicBrainzTrackMetadata {
  trackId: string | null;
  recordingId: string | null;
  position: number;
  number: string;
  title: string;
  artistCredit: string;
  artistIds: string[];
}

export interface MusicBrainzSelection {
  releaseId: string;
  releaseGroupId: string | null;
  releaseTitle: string;
  albumArtist: string;
  albumArtistIds: string[];
  releaseDate: string | null;
  originalYear: string | null;
  label: string | null;
  catalogNumber: string | null;
  mediumPosition: number;
  mediumTitle: string | null;
  mediumTrackCount: number;
  totalMedia: number;
  chapterTracks: Array<MusicBrainzTrackMetadata | null>;
}

export interface ImportedChapters {
  editions: Edition[];
  advanced: boolean;
  advancedReason: string | null;
  originalXml: string;
}

export interface ToolStatus {
  available: boolean;
  directory: string | null;
  version: string | null;
  majorVersion: number | null;
  missing: string[];
  searched: string[];
  message: string;
}

export interface MediumSummary {
  id: string | null;
  position: number;
  title: string | null;
  format: string | null;
  trackCount: number;
}

export interface ReleaseCandidate {
  id: string;
  title: string;
  artistCredit: string;
  date: string | null;
  country: string | null;
  status: string | null;
  score: number;
  media: MediumSummary[];
}

export interface TrackInfo {
  id: string | null;
  position: number;
  number: string;
  title: string;
  lengthMs: number | null;
  recordingId: string | null;
  artistCredit: string;
  artistIds: string[];
}

export interface MediumTracklist {
  id: string | null;
  position: number;
  title: string | null;
  format: string | null;
  tracks: TrackInfo[];
}

export interface ReleaseDetails {
  id: string;
  title: string;
  artistCredit: string;
  artistIds: string[];
  date: string | null;
  originalYear: string | null;
  releaseGroupId: string | null;
  label: string | null;
  catalogNumber: string | null;
  media: MediumTracklist[];
}

export interface AudioStream {
  index: number;
  codec: string;
  codecLongName: string;
  title: string | null;
  language: string | null;
  channels: number;
  channelLayout: string | null;
  sampleRate: number;
  bitDepth: number;
  isDefault: boolean;
}

export interface FlacProbe {
  available: boolean;
  ffmpegPath: string | null;
  version: string | null;
  streams: AudioStream[];
  searched: string[];
  message: string;
}

export interface FlacFallbackMetadata {
  albumArtist: string;
  album: string;
  releaseYear: string;
  originalYear: string;
  label: string;
  catalogNumber: string;
}

export interface FlacExtractionRequest {
  document: ChapterDocument;
  stream: AudioStream;
  outputParent: string;
  fallback: FlacFallbackMetadata;
}

export interface FlacTrackPreview {
  chapterIndex: number;
  relativePath: string;
  title: string;
  artist: string;
  trackNumber: string;
  startNs: number;
  endNs: number;
}

export interface FlacExtractionPlan {
  albumDirectory: string;
  usesMusicbrainz: boolean;
  tracks: FlacTrackPreview[];
  warnings: string[];
}

export interface FlacProgress {
  phase: string;
  completed: number;
  total: number;
  filename: string;
}

export interface FlacExtractionResult {
  outputDirectory: string;
  files: string[];
  warnings: string[];
}

export interface SaveVerification {
  targetPath: string;
  recoveryPath: string | null;
  warnings: string[];
  verified: boolean;
  document: ChapterDocument;
}

export interface SaveProgress {
  phase: string;
  completed: number;
  total: number;
}
