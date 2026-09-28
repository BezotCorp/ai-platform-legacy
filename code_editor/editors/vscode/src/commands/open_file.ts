import * as vscode from 'vscode';
import type { OpenFileRequest } from '../types';
import { toVsCodeRange } from './conversion';

function requestUri(request: OpenFileRequest): vscode.Uri {
  if (request.uri !== undefined) {
    return vscode.Uri.parse(request.uri);
  }
  if (request.path !== undefined) {
    return vscode.Uri.file(request.path);
  }
  throw new Error('open_file requires uri or path');
}

export async function openFile(request: OpenFileRequest): Promise<void> {
  const document = await vscode.workspace.openTextDocument(requestUri(request));
  const options: vscode.TextDocumentShowOptions = {};
  if (request.selection !== undefined) {
    options.selection = toVsCodeRange(request.selection);
  }
  await vscode.window.showTextDocument(document, options);
}
