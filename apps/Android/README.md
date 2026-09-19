# Syntaxis for Android

Android uses the shared server UI in a WebView, with a mandatory local backend in
Termux and an optional HTTPS remote server. It never uses `runtime-browser`.
Projects stay on their owning backend; the app combines their recent-project
lists without copying files or credentials between backends.

Modern ARM64 Android devices are the primary target. Android 8+ and a current
Android System WebView with secure web-message support are required. Nexus 7
(2013), ARMv7, Android 11 remains best effort. Remote editing and Local login were
confirmed on that tablet before the integrated-home update; the new onboarding,
pairing, merged list, and destination switch still need device acceptance.

## First run

1. Install Termux from its official F-Droid or GitHub distribution.
2. In Termux, run `pkg update` and `termux-setup-storage`.
3. Download `install-syntaxis.sh`, the backend archive for your architecture, and
   its `.sha256` file from the same GitHub release into Downloads. ARM64 uses
   `syntaxis-termux-arm64-v8a.tar.gz`; Nexus 7 uses `syntaxis-termux-armeabi-v7a.tar.gz`.
4. Run `bash ~/storage/downloads/install-syntaxis.sh --no-start` in Termux.
5. Install `syntaxis.apk`, open Syntaxis, and tap **Start and pair**. Grant the Run
   commands permission when asked. Return after Termux shows the running backend.
6. On the welcome screen, enter a remote server URL and password, or **Skip**.

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

Stop the backend with Ctrl+C in Termux before updating. Copy the new
installer/archive/checksum and run:

```sh
bash ~/storage/downloads/install-syntaxis.sh --no-start
```

Then reopen Syntaxis and tap **Start and pair**. Subsequent manual starts can use
`bash ~/.local/share/syntaxis/start.sh`; pairing survives ordinary restarts.
Updates refuse to replace a running backend. Private Pi versions and versioned
backend releases live in `~/.local/share/syntaxis`; activation switches a `current`
symlink. Legacy flat installations are migrated with a previous-release copy.
Projects, credentials, state, and global Pi are preserved. Old releases are retained.

To restore the previous backend:

```sh
bash ~/storage/downloads/install-syntaxis.sh --rollback --no-start
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
README into `target/android-distribution`. Never compile the workspace on a 2 GB
tablet. Archives include matching server/frontend assets, ABI, Pi version, and
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
existing Release Please workflow when a release is created. It builds and checks
a signed release APK and both backend ABIs, verifies the source version and tag
commit, and uploads `syntaxis.apk`, both archives and sidecar checksums,
`install-syntaxis.sh`, `Android-README.md`, and `Android-SHA256SUMS` to that release.
It does not create a new release or publish an unsigned/debug APK.

A maintainer must create a long-lived signing key and configure these repository
Actions secrets once:

| Secret | Value |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | Base64-encoded release keystore |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | Signing-key alias |
| `ANDROID_KEY_PASSWORD` | Signing-key password |

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
The user deferred the broader tablet workflow checks; do not report them as passed.
