import * as vscode from 'vscode';
import type { WorkspaceRoot } from '../types';
import { filePath } from './conversion';

export async function workspaceRoot(): Promise<WorkspaceRoot | null> {
  const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
  if (workspaceFolder === undefined) {
    return null;
  }
  return {
    editor: 'vscode',
    uri: workspaceFolder.uri.toString(),
    path: filePath(workspaceFolder.uri),
    name: workspaceFolder.name,
  };
}
