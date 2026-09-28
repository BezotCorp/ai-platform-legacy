export interface EditorPosition {
  line: number;
  character: number;
}

export interface EditorRange {
  start: EditorPosition;
  end: EditorPosition;
}

export interface CurrentFile {
  editor: 'vscode';
  uri: string;
  path: string | null;
  workspaceRoot: string | null;
  languageId: string;
  version: number;
  isDirty: boolean;
  text: string;
}

export interface CurrentSelection {
  editor: 'vscode';
  uri: string;
  path: string | null;
  range: EditorRange;
  text: string;
  isEmpty: boolean;
}

export interface WorkspaceRoot {
  editor: 'vscode';
  uri: string;
  path: string | null;
  name: string;
}

export interface OpenFileRequest {
  uri?: string;
  path?: string;
  selection?: EditorRange;
}

export interface TextEdit {
  range: EditorRange;
  replacement: string;
}

export interface ApplyEditRequest {
  uri?: string;
  path?: string;
  edits: TextEdit[];
  save?: boolean;
}

export interface ApplyEditResult {
  applied: boolean;
  saved: boolean;
}

export interface VscodeEditorApi {
  currentFile(): Promise<CurrentFile | null>;
  currentSelection(): Promise<CurrentSelection | null>;
  workspaceRoot(): Promise<WorkspaceRoot | null>;
  openFile(request: OpenFileRequest): Promise<void>;
  applyEdit(request: ApplyEditRequest): Promise<ApplyEditResult>;
}
