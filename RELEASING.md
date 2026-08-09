# Releasing Purrch

Two signatures matter, and they are unrelated to each other:

1. **The updater signature** (minisign). Already set up. Without the matching
   private key you cannot ship an update that existing installs will accept.
2. **Authenticode code signing** (a real certificate). **Not set up.** Until it
   is, every user who downloads Purrch gets a red SmartScreen wall.

## 1. The updater key — set up, back it up

A keypair was generated at:

```
%USERPROFILE%\.purrch\updater.key       <- private. back this up.
%USERPROFILE%\.purrch\updater.key.pub   <- public
```

The public half is in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
It has no password.

**Back the private key up somewhere you will still have in two years.** If it is
lost, every copy of Purrch already installed becomes permanently unupdatable —
they will refuse anything signed by a different key, and the only route left is
asking every user to download a fresh installer by hand. The public key is
already baked into shipped binaries; you cannot change your mind later.

Before the first public release, consider regenerating it *with* a password:

```sh
npx tauri signer generate -w "$USERPROFILE/.purrch/updater.key"
```

and put the password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Fine to do now,
impossible to do after anyone has installed 0.1.0.

For CI, put the file's *contents* in a repository secret:

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | contents of `updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password, or `""` |

## 2. Authenticode — the one still outstanding

Nothing in this repo signs the installer, so `Purrch_x.y.z_x64-setup.exe` is
unsigned. Windows shows "Windows protected your PC — Unknown publisher", and the
user has to click through *More info → Run anyway*. For an app whose own consent
screen says it can run any command on your machine, that is the worst possible
first impression: the exact warning a user should heed is the one you are asking
them to dismiss.

Two ways out:

- **Azure Trusted Signing** — roughly $10/month, no hardware token, works from
  CI. The reasonable default for a solo project. Requires an Azure account and
  an identity validation that takes a few days.
- **An OV certificate from a CA** — a few hundred a year, arrives on a hardware
  token, which makes CI signing awkward. EV buys instant SmartScreen reputation;
  OV builds it over time and downloads.

Either way, reputation is per-certificate: the first few hundred downloads will
still warn even once signed.

Once you have one, add to `bundle.windows` in `src-tauri/tauri.conf.json`:

```jsonc
"windows": {
  "signCommand": "trusted-signing-cli -e <endpoint> -a <account> -c <profile> %1",
  "nsis": { "installerHooks": "installer-hooks.nsh" }
}
```

Do not commit the certificate, its password, or a token PIN.

## 3. Cutting a release

1. Bump the version in **both** `package.json` and `src-tauri/tauri.conf.json`.
   They must match — the updater compares the running build against what the
   manifest advertises.
2. Note what changed. The updater shows the release body to the user.
3. Tag and push:

   ```sh
   git tag v0.1.1
   git push origin v0.1.1
   ```

4. `.github/workflows/release.yml` builds the NSIS installer, signs it with the
   updater key, and creates a **draft** GitHub release containing the `.exe`,
   its `.sig`, and `latest.json`.
5. Download the installer from the draft, **install it on a clean machine, and
   run it.** CI proves it compiles; it does not prove a cat appears on the
   taskbar.
6. Publish the release. `plugins.updater.endpoints` points at
   `releases/latest/download/latest.json`, so publishing is what actually ships
   it. A draft release is invisible to the updater, which is the point.

## 4. Before the first public release

Things CI cannot check and a compile cannot tell you:

- [ ] `cargo test -- --ignored` on a real machine — these cover the microphone
      and the OS keychain, and a silent keychain failure means keys quietly
      landing in a file instead.
- [ ] Install, run a turn, write a chore, watch a hunt fire, open the gift and
      unfold its trail.
- [ ] Uninstall, and confirm `%APPDATA%\fun.purrch.pet` is gone.
- [ ] Test the update path itself: install the previous version, publish this
      one, take the update from the panel. The updater is the one feature that
      cannot be fixed by an update.
- [ ] Drive the **Codex** backend once, or keep it marked untested. It is
      marked untested in `detect.rs` and the panel says so; that flag is a
      promise about what has actually been verified, so only clear it after
      someone has watched a real Codex turn work.
