# ZhiForge Browser Helper

This helper adds structured Zhihu source context to the existing ZhiForge desktop selection flow.

It does **not** save knowledge, call AI models, manage review state, or replace the Windows selection detector. It only sends the current Zhihu selection metadata to the local desktop bridge so that a later click on **内化** can save a richer Source.

## Chromium browsers

Supported for the MVP: Chrome, Edge, Vivaldi and other Chromium browsers.

1. Start the ZhiForge desktop app.
2. Open the browser extensions page and enable Developer mode.
3. Choose **Load unpacked** and select this `browser-extension` directory.
4. Open a Zhihu question/answer page and select text normally.
5. Use the existing ZhiForge floating toolbar and click **内化**.

No API key or manual token is stored in the extension. The extension automatically pairs with `127.0.0.1:17832` using a random desktop token that expires whenever ZhiForge restarts.

## Firefox development manifest

`manifest.firefox.json` contains the equivalent Firefox background-script declaration. For temporary Firefox testing, use a copy of this directory with `manifest.firefox.json` renamed to `manifest.json`.

Generic ZhiForge desktop selection continues to work in Firefox even without the helper; only structured Zhihu URL/author/context enrichment depends on the helper.

## Architecture

- `zhihu-adapter.js`: all Zhihu DOM parsing and selectors.
- `content.js`: selection observation only.
- `background.js`: validates the sender and forwards context.
- `desktop-bridge.js`: localhost pairing and authenticated `/capture` calls.

The Zhihu selectors intentionally stay inside `zhihu-adapter.js` so a Zhihu DOM redesign does not spread selector changes through business code.

## Local security boundary

The desktop bridge:

- binds only to `127.0.0.1:17832`;
- accepts pairing only from `chrome-extension://` or `moz-extension://` origins;
- requires the `X-ZhiForge-Client` header;
- issues a random in-memory bearer token;
- requires that token for `/capture`;
- validates that capture URLs are HTTPS Zhihu URLs;
- stores the context only briefly until the matching desktop selection is internalized.
