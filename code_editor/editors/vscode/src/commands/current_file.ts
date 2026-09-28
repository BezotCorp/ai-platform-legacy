import * as vscode from 'vscode';
import type { CurrentFile } from '../types';
import { filePath } from './conversion';

export async function currentFile(): Promise<CurrentFile | null> {
  const editor = vscode.window.activeTextEditor;
  if (editor === undefined) {
    return null;
  }
  const document = editor.document;
  const workspace = vscode.workspace.getWorkspaceFolder(document.uri);
  return {
    editor: 'vscode',
    uri: document.uri.toString(),
    path: filePath(document.uri),
    workspaceRoot: workspace === undefined ? null : filePath(workspace.uri),
    languageId: document.languageId,
    version: document.version,
    isDirty: document.isDirty,
    text: document.getText(),
  };
}
