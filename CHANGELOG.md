# Changelog

All notable public changes to ZhiForge are documented here.

The project follows Semantic Versioning from v1.0.0 onward.

## [1.4.0] - Unreleased

### Changed
- ZhiForge-owned code is now source-available under the PolyForm Noncommercial License 1.0.0.
- Commercial or business use of v1.4.0 and later requires a separate written commercial license.
- Prior releases through v1.3.0 remain under their original AGPL-3.0-only grants; the license change is prospective and does not revoke prior grants.

## [1.1.3] - 2026-09-14

### Fixed
- Corrects Zhihu search IPC serialization so Rust emits the camelCase fields expected by the React renderer instead of API-side PascalCase names.
- Adds an IPC regression assertion for the Zhihu search result contract.
- Isolates Knowledge Workspace rendering failures so selection-action listeners remain mounted instead of blanking the entire result window.

## [1.1.2] - 2026-09-14

### Fixed
- Restores minimized result windows through the Windows native restore path plus Tauri state recovery, so toolbar actions no longer appear unresponsive after minimizing the main window.
- Brings restored result windows to the foreground for home, Zhihu search, translation, explanation and other selection actions.
- Zhihu search now delivers the pending search state before showing the main window, avoiding the old action blur/hide race.

## [1.1.1] - 2026-09-14

### Fixed
- The model picker now shows the complete fetched model list in a scrollable selector instead of relying on the native datalist UI.
- Selection toolbar Zhihu search now reliably restores the main window and carries the pending search request into the Zhihu search view.

## [1.0.0] - 2026-09-11

First public stable release.

### Added
- Windows global text-selection toolbar and result window.
- Translation, explanation, polishing and dynamic custom AI actions.
- Provider profiles, model routing, fallback routes and prompt templates.
- OpenAI Compatible, OpenAI Responses, Anthropic and Gemini adapters.
- Model capability handling for sampling and reasoning options.
- Custom headers and model-list retrieval.
- Application rules and clipboard fallback compatibility.
- Windows Credential Manager-backed API key storage.
- Diagnostics logging without selected text, prompts or API keys.
- Settings center with provider, action, selection, app-rule and general pages.
- Minimize / maximize support and separate compact selection-result sizing.

### Fixed
- Chinese toolbar labels no longer collapse vertically.
- Single-provider API failures preserve real error kind and HTTP status.
- API key storage now uses the Windows-native keyring backend and verifies round-trip persistence.
- Settings and home windows open at a usable default size.

### Notes
- Internal `0.x` builds were development snapshots and are not part of the public release history.
