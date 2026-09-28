import * as vscode from 'vscode';
import type { CurrentSelection } from '../types';
import { filePath, toEditorRange } from './conversion';

export async function currentSelection(): Promise<CurrentSelection | null> {
  const editor = vscode.window.activeTextEditor;
  if (editor === undefined) {
    return null;
  }
  const document = editor.document;
  const selection = editor.selection;
  return {
    editor: 'vscode',
    uri: document.uri.toString(),
    path: filePath(document.uri),
    range: toEditorRange(selection),
    text: document.getText(selection),
    isEmpty: selection.isEmpty,
  };
}
