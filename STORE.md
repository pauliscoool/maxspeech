# MaxSpeech — Microsoft Store publishing

SmartScreen shows “Windows protected your PC” for **unsigned** direct downloads (e.g. the Vercel `.exe`). There is no supported bypass without a trusted publisher signature.

## Recommended path (no Authenticode cert purchase)

Publish MaxSpeech through the **Microsoft Store**. Store packages are re-signed by Microsoft and do **not** show SmartScreen download warnings.

### Steps

1. Create a [Microsoft Partner Center](https://partner.microsoft.com/dashboard) developer account (small one-time fee for individuals — this is **not** a code-signing certificate product).
2. Build an MSIX / Store package from this project:
   ```bash
   npm run tauri build -- --bundles msi
   ```
   Prefer packaging as MSIX when your Tauri/Windows toolchain supports Store targets; otherwise submit the Win32 installer per Partner Center “EXE/MSI” product type (Store still wraps distribution).
3. In Partner Center: create the MaxSpeech app listing, upload the package, fill Store listing (description, screenshots, age rating, privacy policy URL: `https://maxspeech.vercel.app/privacy.html`).
4. Submit for certification. After approval, users install from the Store with no SmartScreen prompt.

### Until Store is live

- Keep distributing `MaxSpeech_*_x64-setup.exe` from https://maxspeech.vercel.app
- Tell users: SmartScreen → **More info** → **Run anyway**
- If Defender falsely flags the file, submit it at https://www.microsoft.com/wdsi/filesubmission

### Out of scope here

Buying DigiCert/Sectigo/EV Authenticode certificates. Azure Artifact Signing (~$10/mo) is an alternative for non-Store EXE distribution if you later want signed direct downloads.
