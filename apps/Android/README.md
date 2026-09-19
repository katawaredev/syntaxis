# Syntaxis for Android

Android uses the shared server UI in a WebView, with a mandatory local backend in
Termux and an optional HTTPS remote server. It never uses `runtime-browser`.
Projects stay on their owning backend; the app combines their recent-project
lists without copying files or credentials between backends.

Modern ARM64 Android devices are the primary target. Android 8+ and a current
Android System WebView with secure web-message support are required. Older ARMv7
devices are supported on a best-effort basis. The integrated onboarding, pairing,
merged list, and destination switch still need device acceptance.

## First run

1. Install `syntaxis.apk` from a Syntaxis GitHub release, or install the repository
   through Komi Store. Install Termux from its official F-Droid or GitHub distribution.
2. In Termux, run `pkg update && pkg install curl`.
3. Open Syntaxis, tap **Copy setup command**, and run it in Termux. Alternatively:

   ```sh
   curl --fail --location --proto '=https' --proto-redir '=https' \
     https://github.com/katawaredev/syntaxis/releases/latest/download/install-syntaxis.sh \
     -o "$HOME/syntaxis-install.sh" &&
   bash "$HOME/syntaxis-install.sh" --latest --no-start
   ```

4. Return to Syntaxis and tap **Start and pair**. Grant the Run commands permission
   when asked, then return after Termux shows the running backend.
5. On the welcome screen, enter a remote server URL and password, or **Skip**.

The download command becomes available when the first release containing these
Android assets finishes publishing. It selects the device ABI automatically and
pins all downloads to one release. Downloads use Termux private storage; no shared
storage permission or manual archive transfer is needed. An incomplete release
fails without replacing the installed backend; retry after Publish Android finishes.

For offline/manual installation, copy `install-syntaxis.sh`, the matching archive,
and its `.sha256` sidecar into Termux, then pass the archive path to the installer.
Use `syntaxis-termux-arm64-v8a.tar.gz` for ARM64 or
`syntaxis-termux-armeabi-v7a.tar.gz` for ARMv7. Shared Downloads access is optional
and requires `termux-setup-storage` only when using that transfer method.

The installer checks the archive checksum and ABI, installs missing tools and a
private version of Pi, and enables Termux's `allow-external-apps=true` setting.
Pi needs Node 22.19+; Node 26 works. The APK checks authenticated local readiness
before opening any projects, including remote projects. Missing setup produces
instructions instead of a partly working app.

No local password entry is required. Pairing passes a random app token through
Termux's explicit RUN_COMMAND service. The backend exchanges that token for an
HttpOnly WebView session cookie. Release authentication remains enabled, bound
only to `127.0.0.1:8787`; there is no unauthenticated loopback bypass. Existing local
password hashes are retained, and fresh installations generate a random one.
Clearing APK data requires pairing again with a stopped backend.

## Projects and connections

- There is no native toolbar. The shared home page combines Local and Remote
  recents, sorted by last-opened time and labelled by location. Identical names
  and IDs remain separate because each entry routes to its own backend.
- New project and Clone dialogs default to Local. When a remote is configured,
  **Local project** can be unchecked to use it. Without a remote, the control is
  hidden and all new projects are local. Choose the destination before entering
  the form: switching loads that backend's dialog.
- **Recent projects → ⋮ → Add Remote** opens connection setup after skipping it.
  Once configured, **Remote settings** reconnects, replaces, or removes it.
  Removing a connection does not delete projects. Remote passwords are used for
  login and are not persisted; only origin-specific session cookies are stored.
- An unavailable remote produces a notice alongside working local projects.
  Expired remote sessions can be renewed in Remote settings. Remote servers must
  run a version with the Android shell integration and `/new-project` and
  `/clone-project` routes.
- Local initially offers empty projects. Install tools through Termux and run
  scaffolding in the terminal. Desktop Mise templates and automatic LSP setup
  are not advertised on Android.
- Backend selection navigates a single WebView. Save edits before leaving a
  project; unsaved UI state is not transferred. Android Back returns to projects.

The narrowly scoped native message channel accepts only top-level app home and
creation pages from the local origin and configured HTTPS origin. Preview frames
cannot invoke it. It supports connection metadata, remote recents, fixed app
navigation, and opening connection setup—not shell execution or credential access.
No `addJavascriptInterface` bridge is used. TLS verification is never disabled.

## Updates and rollback

Komi Store can install the APK from GitHub and track APK updates. Its optional
silent/automatic installation features depend on its own device setup. See
[Komi's features and discovery requirements](https://komistore.app/features/).
The published APK keeps one application ID, a stable filename, increasing version
codes, and a consistent signing identity. No separate Komi submission is required
for a public repository with a published APK release; relevant repository topics
and description help discovery.

Komi's APK updates do not install the separate Termux backend. After updating the
APK, stop the backend with Ctrl+C in Termux and run:

```sh
bash ~/.local/share/syntaxis/update.sh --latest --no-start
```

The installer saves this updater after each successful installation. Existing
installations without `update.sh` should run the first-run download command once.
To install the exact release matching an APK instead of the latest stable one:

```sh
bash ~/.local/share/syntaxis/update.sh --release=v0.13.0 --no-start
```

Replace the example tag with the installed APK's release version. The downloader
checks the release installer's checksum before running it, and the installer
checks the archive's checksum and ABI. Package-manager prompts and Termux setup
remain necessary; Komi's APK update flow does not run the backend installer.

Then reopen Syntaxis and tap **Start and pair**. Subsequent manual starts can use
`bash ~/.local/share/syntaxis/start.sh`; pairing survives ordinary restarts.
Updates refuse to replace a running backend. Private Pi versions and versioned
backend releases live in `~/.local/share/syntaxis`; activation switches a `current`
symlink. Legacy flat installations are migrated with a previous-release copy.
Projects, credentials, state, and global Pi are preserved. Old releases are retained.

To restore the previous backend:

```sh
bash ~/.local/share/syntaxis/update.sh --rollback --no-start
```

Rollback does not undo project or state changes. A backend from before app-token
pairing needs its matching older APK. APK and backend updates are separate; install
new APKs over the old APK using the same signing key. A release APK cannot update
a development APK signed with a different debug key. Export any important APK
settings before uninstalling a differently signed APK; Termux data is separate.

## Build on a development computer

Requires JDK 17+, Android SDK platform 36.1, build tools 36.0.0, and the repository's
Rust/Bun/Dioxus tools. Gradle and its checksum are pinned. Set `ANDROID_HOME` and
`JAVA_HOME` as appropriate:

```sh
apps/Android/gradlew -p apps/Android :app:assembleDebug :app:testDebugUnitTest :app:lintDebug
python3 apps/Android/termux/test-install.py
python3 apps/Android/termux/test-start.py
```

APK: `apps/Android/app/build/outputs/apk/debug/app-debug.apk`. Install with
`adb install -r` or Android's package installer. Version name comes from
`apps/server/Cargo.toml`; version code is `major * 1000000 + minor * 1000 + patch`.

On Linux x86-64, install Android NDK r28+ and the Rust targets, then build backend
ABIs **sequentially** (Dioxus shares its output directory):

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi
export ANDROID_NDK_HOME=/absolute/path/to/ndk/28.2.13676358
bash apps/Android/termux/build-backend.sh arm64-v8a
bash apps/Android/termux/build-backend.sh armeabi-v7a
bash apps/Android/package.sh
```

The last command collects the debug APK, both archives/checksums, installer, and
README into `target/android-distribution`. Build on a development computer,
not on the Android device. Archives include matching server/frontend assets, ABI, Pi version, and
source revision. The [narrow Manganis patch](termux/vendor/README.md) permits an
ARMv7 headless backend without claiming 32-bit Dioxus native-renderer support.
The shared-home smoke test uses an isolated debug backend, temporary project
storage, and a simulated native port:

```sh
dx build --package syntaxis-server --platform web --debug-symbols false
AUTORESEARCH_BROWSER_PATH=/path/to/chromium bun apps/Android/test-home.mjs
```

CI runs this test along with APK checks and installer/pairing tests. It does not
replace actual Termux/WebView acceptance. Follow root AGENTS.md before launching
repository QA in an agent session.

## Publish to GitHub Releases

[Publish Android](../../.github/workflows/publish-android.yml) is called by the
existing Release Please workflow when a release is created. Docker and Android
publishing both depend on the same release job and receive the same version and
commit. They start in the same pipeline and finish independently; merging the
Release Please PR is the normal publishing action for both.

The Android job builds and checks a signed release APK and both backend ABIs, verifies the source version and tag
commit, and uploads `syntaxis.apk`, both archives and sidecar checksums,
`install-syntaxis.sh`, `Android-README.md`, and `Android-SHA256SUMS` to that release.
It does not create a new release or publish an unsigned/debug APK.

Android requires a signed APK even when distributing only through GitHub.
Use your own app-signing key; Google Play, a Play upload key, and a store account
are not part of this workflow. This key is separate from Git/GPG commit signing.
See [Android app signing](https://developer.android.com/studio/publish/app-signing).

Generate a key once on your development computer (`keytool` ships with the JDK):

```sh
umask 077
mkdir -p "$HOME/.local/share/syntaxis-signing"
keytool -genkeypair -v -storetype PKCS12 \
  -keystore "$HOME/.local/share/syntaxis-signing/syntaxis-release.p12" \
  -alias syntaxis -keyalg RSA -keysize 3072 -validity 10000
```

Choose a strong password at the prompt and supply certificate identity details.
The PKCS12 key uses the keystore password; set both password secrets below to that
same value. Keep this file outside the repository and retain an offline backup.
Do not generate a new key for every release.

A maintainer must configure these repository Actions secrets once:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | Base64-encoded release keystore |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | Signing-key alias |
| `ANDROID_KEY_PASSWORD` | Signing-key password |

In GitHub, use **Settings → Secrets and variables → Actions → New repository
secret**, or use an authenticated GitHub CLI (password commands prompt):

```sh
base64 -w 0 "$HOME/.local/share/syntaxis-signing/syntaxis-release.p12" |
  gh secret set ANDROID_KEYSTORE_BASE64 --repo katawaredev/syntaxis
gh secret set ANDROID_KEY_ALIAS --repo katawaredev/syntaxis --body syntaxis
gh secret set ANDROID_KEYSTORE_PASSWORD --repo katawaredev/syntaxis
gh secret set ANDROID_KEY_PASSWORD --repo katawaredev/syntaxis
```

The `base64 -w 0` syntax is for GNU/Linux. The keystore's Base64 value belongs in
an Actions secret, never in a release asset or source file.

Keep an offline backup of the keystore and passwords: updates require the same
signing identity. Missing secrets fail publishing rather than generating a new
key. Keys are decoded only into the runner's temporary directory and removed
with an always-run cleanup step. No signing material belongs in this repository.

To retry publishing an existing release, run **Actions → Publish Android → Run
workflow**, providing its version without `v` and matching tag/commit. Uploads
replace matching Android asset names only. The workflow has not yet been executed
on GitHub; local build checks do not establish that a release was published.

## Persistence and limits

Projects live in Termux `~/Projects`; registry/state is in
`~/.local/state/syntaxis`, authentication in `~/.config/syntaxis`. Pi uses its
normal Termux-local credentials and sessions. These remain separate from remote
servers and browser-only in-memory AI state. Removing Termux removes its private
data unless backed up. The APK stores a local pairing token and connection
preferences in private app storage, with Android backup disabled.

Keep Termux running. Android may kill background processes, and reconnection does
not recreate a killed terminal. Cloud AI, clones, and package installation still
need network access. Local files, shell, and Git do not need an external host.
File inputs use Android's picker. Downloads use the browser; blob exports still
need a browser session. Previews, Pi OAuth callbacks, low-memory recovery, and
long background jobs need device testing. Docker, desktop binary installers, and
arbitrary native npm modules are not presumed to work on Android.

Acceptance should cover first-run setup, skip/add/remove remote, mixed same-name
projects, creation/cloning in each destination, expired cookies, restart/pairing,
file edits, terminal/Git/Pi, background/rotation, and remote-offline behavior.
Broader on-device workflow checks remain outstanding.
