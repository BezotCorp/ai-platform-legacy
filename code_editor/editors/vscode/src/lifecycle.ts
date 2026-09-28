import * as vscode from 'vscode';
import { createVsCodeEditorApi } from './api';

export function activate(context: vscode.ExtensionContext): void {
  const api = createVsCodeEditorApi();

  context.subscriptions.push(
    vscode.commands.registerCommand('aiPlatform.currentFile', async () => api.currentFile()),
    vscode.commands.registerCommand('aiPlatform.currentSelection', async () => api.currentSelection()),
    vscode.commands.registerCommand('aiPlatform.workspaceRoot', async () => api.workspaceRoot()),
  );
}

export function deactivate(): void {
  // No long-lived transport is started yet.
}
