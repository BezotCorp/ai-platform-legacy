import type {
  ApplyEditRequest,
  ApplyEditResult,
  CurrentFile,
  CurrentSelection,
  OpenFileRequest,
  VscodeEditorApi,
  WorkspaceRoot,
} from './types';
import { applyEdit } from './commands/apply_edit';
import { currentFile } from './commands/current_file';
import { currentSelection } from './commands/current_selection';
import { openFile } from './commands/open_file';
import { workspaceRoot } from './commands/workspace_root';

export class VsCodeEditorApi implements VscodeEditorApi {
  currentFile(): Promise<CurrentFile | null> {
    return currentFile();
  }

  currentSelection(): Promise<CurrentSelection | null> {
    return currentSelection();
  }

  workspaceRoot(): Promise<WorkspaceRoot | null> {
    return workspaceRoot();
  }

  openFile(request: OpenFileRequest): Promise<void> {
    return openFile(request);
  }

  applyEdit(request: ApplyEditRequest): Promise<ApplyEditResult> {
    return applyEdit(request);
  }
}

export function createVsCodeEditorApi(): VscodeEditorApi {
  return new VsCodeEditorApi();
}
