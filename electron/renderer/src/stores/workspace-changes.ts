/**
 * Workspace Changes Store
 *
 * Manages the DiffModal overlay state: open/close, focused file,
 * and prev/next navigation across all files in the session.
 */
import { create } from 'zustand';
import type { GeneratedFile } from '../lib/generated-files';

interface WorkspaceChangesState {
  /** Whether the diff modal is visible. */
  modalOpen: boolean;
  /** Currently focused file in the diff modal. */
  focusedFile: GeneratedFile | null;
  /** All files across all turns (for prev/next navigation). */
  allFiles: GeneratedFile[];
  /** Index of focusedFile in allFiles. */
  focusedFileIndex: number;

  /** Open the diff modal for a specific file, with full file list for nav. */
  openDiff: (file: GeneratedFile, allFiles: GeneratedFile[]) => void;
  /** Close the diff modal. */
  closeDiff: () => void;
  /** Navigate to the next file in the list. */
  nextFile: () => void;
  /** Navigate to the previous file in the list. */
  prevFile: () => void;
}

export const useWorkspaceChangesStore = create<WorkspaceChangesState>(
  (set, get) => ({
    modalOpen: false,
    focusedFile: null,
    allFiles: [],
    focusedFileIndex: -1,

    openDiff: (file, allFiles) => {
      const index = allFiles.findIndex((f) => f.filePath === file.filePath);
      set({
        modalOpen: true,
        focusedFile: file,
        allFiles,
        focusedFileIndex: index >= 0 ? index : 0,
      });
    },

    closeDiff: () => set({ modalOpen: false, focusedFile: null }),

    nextFile: () => {
      const { allFiles, focusedFileIndex } = get();
      if (allFiles.length <= 1) return;
      const next = (focusedFileIndex + 1) % allFiles.length;
      set({
        focusedFile: allFiles[next],
        focusedFileIndex: next,
      });
    },

    prevFile: () => {
      const { allFiles, focusedFileIndex } = get();
      if (allFiles.length <= 1) return;
      const prev =
        (focusedFileIndex - 1 + allFiles.length) % allFiles.length;
      set({
        focusedFile: allFiles[prev],
        focusedFileIndex: prev,
      });
    },
  }),
);
