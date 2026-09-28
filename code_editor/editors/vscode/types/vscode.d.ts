declare module 'vscode' {
  export interface Disposable {
    dispose(): unknown;
  }

  export interface ExtensionContext {
    subscriptions: Disposable[];
  }

  export class Position {
    constructor(line: number, character: number);
    readonly line: number;
    readonly character: number;
  }

  export class Range {
    constructor(start: Position, end: Position);
    readonly start: Position;
    readonly end: Position;
  }

  export class Selection extends Range {
    readonly isEmpty: boolean;
  }

  export class Uri {
    static parse(value: string): Uri;
    static file(path: string): Uri;
    readonly scheme: string;
    readonly fsPath: string;
    toString(): string;
  }

  export interface TextDocument {
    readonly uri: Uri;
    readonly languageId: string;
    readonly version: number;
    readonly isDirty: boolean;
    getText(range?: Range): string;
    save(): Thenable<boolean>;
  }

  export interface TextEditor {
    readonly document: TextDocument;
    readonly selection: Selection;
  }

  export interface WorkspaceFolder {
    readonly uri: Uri;
    readonly name: string;
  }

  export interface TextDocumentShowOptions {
    selection?: Range;
  }

  export class WorkspaceEdit {
    replace(uri: Uri, range: Range, newText: string): void;
  }

  export namespace window {
    export const activeTextEditor: TextEditor | undefined;
    export function showTextDocument(
      document: TextDocument,
      options?: TextDocumentShowOptions,
    ): Thenable<TextEditor>;
  }

  export namespace workspace {
    export const workspaceFolders: readonly WorkspaceFolder[] | undefined;
    export function getWorkspaceFolder(uri: Uri): WorkspaceFolder | undefined;
    export function openTextDocument(uri: Uri): Thenable<TextDocument>;
    export function applyEdit(edit: WorkspaceEdit): Thenable<boolean>;
  }

  export namespace commands {
    export function registerCommand(
      command: string,
      callback: (...args: unknown[]) => unknown,
    ): Disposable;
  }
}
