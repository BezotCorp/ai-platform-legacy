import * as vscode from 'vscode';
import type {
  ApplyEditRequest,
  ApplyEditResult,
} from '../types';
import { toVsCodeRange } from './conversion';

function requestUri(request: ApplyEditRequest): vscode.Uri {
  if (request.uri !== undefined) {
    return vscode.Uri.parse(request.uri);
  }
  if (request.path !== undefined) {
    return vscode.Uri.file(request.path);
  }
  throw new Error('apply_edit requires uri or path');
}

export async function applyEdit(request: ApplyEditRequest): Promise<ApplyEditResult> {
  const uri = requestUri(request);
  const edit = new vscode.WorkspaceEdit();
  for (const change of request.edits) {
    edit.replace(uri, toVsCodeRange(change.range), change.replacement);
  }
  const applied = await vscode.workspace.applyEdit(edit);
  let saved = false;
  if (applied && request.save === true) {
    const document = await vscode.workspace.openTextDocument(uri);
    saved = await document.save();
  }
  return { applied, saved };
}
