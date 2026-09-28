import * as vscode from 'vscode';
import type { EditorRange } from '../types';

export function toEditorRange(range: vscode.Range): EditorRange {
  return {
    start: {
      line: range.start.line,
      character: range.start.character,
    },
    end: {
      line: range.end.line,
      character: range.end.character,
    },
  };
}

export function toVsCodeRange(range: EditorRange): vscode.Range {
  return new vscode.Range(
    new vscode.Position(range.start.line, range.start.character),
    new vscode.Position(range.end.line, range.end.character),
  );
}

export function filePath(uri: vscode.Uri): string | null {
  return uri.scheme === 'file' ? uri.fsPath : null;
}
