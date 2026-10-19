# Updates

## Today: a notice

At most once a day, the app asks GitHub for the latest release (`api.github.com/repos/PoUpA/totalKONTROL/releases/latest`, nothing else is sent). When a newer version is out, a banner offers to open the releases page; **Never check** turns this off for good.

## Next: installing updates by itself

Tauri's updater downloads and installs a new version on its own, but only versions signed with the project's key, so a hijacked download cannot pass for an update. That key must be created and kept by the maintainer: it never goes in the repository.

1. Create the key pair, with a password:

   ```bash
   npx tauri signer generate -w ~/.tauri/totalkontrol.key
   ```

2. In the GitHub repository, under **Settings › Secrets and variables › Actions**, add the secrets `TAURI_SIGNING_PRIVATE_KEY` (the content of `~/.tauri/totalkontrol.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Then, in the code:
   - add `tauri-plugin-updater` (Rust) and `@tauri-apps/plugin-updater`, with the `updater:default` permission;
   - in `tauri.conf.json`, set `bundle.createUpdaterArtifacts` to `true` and add `plugins.updater` with the **public** key (`~/.tauri/totalkontrol.key.pub`) and the endpoint `https://github.com/PoUpA/totalKONTROL/releases/latest/download/latest.json`;
   - in `.github/workflows/release.yml`, pass the two secrets to `tauri-action` as `env`, and set `includeUpdaterJson: true`;
   - replace the notice's **Download** button with the updater's check, download and install.

Code signing (Windows Authenticode, Apple notarization) is a separate matter: it removes the SmartScreen and Gatekeeper warnings, and needs certificates bought in the maintainer's name.
