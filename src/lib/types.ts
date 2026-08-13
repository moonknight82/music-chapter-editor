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
  position: number;
  number: string;
  title: string;
  lengthMs: number | null;
  recordingId: string | null;
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
  media: MediumTracklist[];
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
